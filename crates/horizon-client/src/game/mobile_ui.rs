//! Android-only management surfaces that live inside the game instead of in a
//! launcher. The normal path remains icon -> selected instance -> Minecraft.
//!
//! The first surface is deliberately small: a chip on the pause/title screens
//! opens the instance switcher. Creating, duplicating, renaming and deleting
//! use the same instance storage semantics as Horizon's desktop launcher.
use super::Game;
use crate::{
    android::instances::{self, Instance, Kind},
    hud::{HudBatch, rgba},
    text::Text,
    ui,
};
use std::sync::{Mutex, OnceLock};
use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{KeyCode, PhysicalKey},
};

const PANEL_W: f32 = 310.0;
const ROW_H: f32 = 24.0;
const VISIBLE_ROWS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditKind {
    New,
    Rename,
}

#[derive(Clone, Debug)]
struct Edit {
    kind: EditKind,
    text: String,
}

#[derive(Debug, Default)]
struct State {
    open: bool,
    selected: Option<String>,
    scroll: usize,
    edit: Option<Edit>,
    confirm_delete: Option<String>,
    status: Option<String>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State::default()))
}

fn lock_state() -> std::sync::MutexGuard<'static, State> {
    state().lock().unwrap_or_else(|e| e.into_inner())
}

fn root() -> Option<std::path::PathBuf> {
    instances::root()
}

fn current_name(all: &[Instance]) -> String {
    let id = instances::current_id();
    all.iter()
        .find(|instance| instance.id == id)
        .map(|instance| instance.name.clone())
        .unwrap_or_else(|| "Horizon".into())
}

fn button(
    batch: &mut HudBatch,
    atlas: &crate::hud::HudAtlas,
    cursor: (f32, f32),
    rect: [f32; 4],
    label: impl Into<String>,
    enabled: bool,
) -> bool {
    let [x, y, w, h] = rect;
    let hovered = enabled && ui::hovered(cursor, x, y, w, h);
    ui::button(
        batch,
        atlas,
        x,
        y,
        w,
        h,
        &Text::literal(label.into()),
        enabled,
        hovered,
    );
    hovered
}

impl Game<'_> {
    /// Drawn after the vanilla pause screen. This is intentionally a tiny chip
    /// until opened: management must never turn gameplay into a launcher.
    pub(super) fn draw_android_pause_controls(&mut self, batch: &mut HudBatch) {
        let Some(root) = root() else { return };
        let all = instances::list(&root);
        let atlas = &self.gui.atlas;
        let (w, h) = (batch.width, batch.height);
        let name = current_name(&all);
        let chip_w = (HudBatch::text_width(atlas, &format!("Instance: {name}")) + 20.0)
            .clamp(120.0, 230.0);
        let chip = [8.0, h - 30.0, chip_w, 20.0];
        button(
            batch,
            atlas,
            self.cursor,
            chip,
            format!("Instance: {name}"),
            true,
        );

        let open = lock_state().open;
        if open {
            self.draw_android_instance_panel(batch, &root, &all);
        }
    }

    /// Same compact switcher on the title screen. A build hook calls this after
    /// the vanilla title has drawn; it does not replace Horizon's title UI.
    pub(super) fn draw_android_title_controls(&mut self, batch: &mut HudBatch) {
        let Some(root) = root() else { return };
        let all = instances::list(&root);
        let atlas = &self.gui.atlas;
        let name = current_name(&all);
        let label = format!("{name} ▾");
        let width = (HudBatch::text_width(atlas, &label) + 20.0).clamp(92.0, 220.0);
        button(
            batch,
            atlas,
            self.cursor,
            [8.0, 14.0, width, 20.0],
            label,
            true,
        );
        if lock_state().open {
            self.draw_android_instance_panel(batch, &root, &all);
        }
    }

    fn draw_android_instance_panel(
        &mut self,
        batch: &mut HudBatch,
        root: &std::path::Path,
        all: &[Instance],
    ) {
        let atlas = &self.gui.atlas;
        let (w, h) = (batch.width, batch.height);
        let panel_w = PANEL_W.min(w - 16.0).max(220.0);
        let panel_h = (248.0f32).min(h - 16.0).max(190.0);
        let x = ((w - panel_w) / 2.0).floor();
        let y = ((h - panel_h) / 2.0).floor();

        // Strong enough to separate the manager from the world, but not a
        // second full-screen launcher.
        batch.fill(atlas, x - 2.0, y - 2.0, panel_w + 4.0, panel_h + 4.0, rgba(0, 0, 0, 210));
        batch.fill(atlas, x, y, panel_w, panel_h, rgba(28, 28, 32, 244));
        batch.text(
            atlas,
            "Instances",
            x + 10.0,
            y + 9.0,
            u32::MAX,
            true,
        );
        batch.text(
            atlas,
            "One Horizon runtime · separate worlds, mods and packs",
            x + 10.0,
            y + 21.0,
            rgba(170, 170, 170, 255),
            true,
        );

        let mut state = lock_state();
        if state.selected.is_none() {
            state.selected = Some(instances::current_id());
        }
        let max_scroll = all.len().saturating_sub(VISIBLE_ROWS);
        state.scroll = state.scroll.min(max_scroll);
        let list_top = y + 39.0;
        for (visible, instance) in all
            .iter()
            .skip(state.scroll)
            .take(VISIBLE_ROWS)
            .enumerate()
        {
            let row_y = list_top + visible as f32 * ROW_H;
            let selected = state.selected.as_deref() == Some(instance.id.as_str());
            let current = instance.id == instances::current_id();
            if selected {
                batch.fill(
                    atlas,
                    x + 8.0,
                    row_y,
                    panel_w - 16.0,
                    20.0,
                    rgba(75, 75, 86, 220),
                );
            }
            let prefix = if current { "● " } else { "  " };
            let label = format!("{prefix}{}", instance.name);
            batch.text(
                atlas,
                &label,
                x + 14.0,
                row_y + 6.0,
                if current { u32::MAX } else { rgba(220, 220, 220, 255) },
                true,
            );
            let details = match instance.kind {
                Kind::Default => "Default",
                Kind::Own => "Instance",
            };
            let dw = HudBatch::text_width(atlas, details);
            batch.text(
                atlas,
                details,
                x + panel_w - 14.0 - dw,
                row_y + 6.0,
                rgba(145, 145, 150, 255),
                true,
            );
        }

        if all.len() > VISIBLE_ROWS {
            let scroll_label = format!("{}–{} / {}", state.scroll + 1, (state.scroll + VISIBLE_ROWS).min(all.len()), all.len());
            let tw = HudBatch::text_width(atlas, &scroll_label);
            batch.text(
                atlas,
                &scroll_label,
                x + panel_w - tw - 10.0,
                y + 21.0,
                rgba(150, 150, 155, 255),
                true,
            );
        }

        let actions_y = y + panel_h - 58.0;
        let gap = 4.0;
        let bw = (panel_w - 20.0 - gap * 2.0) / 3.0;
        for (i, label) in ["Switch", "New", "Duplicate"].into_iter().enumerate() {
            button(
                batch,
                atlas,
                self.cursor,
                [x + 10.0 + i as f32 * (bw + gap), actions_y, bw, 20.0],
                label,
                label != "Switch" || state.selected.as_deref() != Some(instances::current_id().as_str()),
            );
        }
        for (i, label) in ["Rename", "Delete", "Close"].into_iter().enumerate() {
            let enabled = if label == "Delete" {
                state
                    .selected
                    .as_deref()
                    .and_then(|id| all.iter().find(|instance| instance.id == id))
                    .is_some_and(|instance| instance.kind != Kind::Default)
            } else {
                true
            };
            button(
                batch,
                atlas,
                self.cursor,
                [x + 10.0 + i as f32 * (bw + gap), actions_y + 24.0, bw, 20.0],
                label,
                enabled,
            );
        }

        if let Some(status) = &state.status {
            let mut status = status.clone();
            if status.chars().count() > 50 {
                status = status.chars().take(47).collect::<String>() + "…";
            }
            batch.text(
                atlas,
                &status,
                x + 10.0,
                actions_y - 11.0,
                rgba(180, 180, 185, 255),
                true,
            );
        }

        if let Some(edit) = &state.edit {
            batch.fill(atlas, x + 18.0, y + 72.0, panel_w - 36.0, 78.0, rgba(5, 5, 7, 245));
            let heading = match edit.kind {
                EditKind::New => "New instance",
                EditKind::Rename => "Rename instance",
            };
            batch.text(atlas, heading, x + 28.0, y + 82.0, u32::MAX, true);
            batch.fill(
                atlas,
                x + 28.0,
                y + 101.0,
                panel_w - 56.0,
                20.0,
                rgba(45, 45, 50, 255),
            );
            batch.text(
                atlas,
                &format!("{}_", edit.text),
                x + 34.0,
                y + 107.0,
                u32::MAX,
                true,
            );
            batch.text(
                atlas,
                "Enter to save · Back/Esc to cancel",
                x + 28.0,
                y + 132.0,
                rgba(150, 150, 155, 255),
                true,
            );
        }

        if let Some(id) = &state.confirm_delete
            && let Some(instance) = all.iter().find(|instance| &instance.id == id)
        {
            batch.fill(atlas, x + 18.0, y + 72.0, panel_w - 36.0, 78.0, rgba(5, 5, 7, 248));
            batch.text(
                atlas,
                &format!("Delete {}?", instance.name),
                x + 28.0,
                y + 84.0,
                u32::MAX,
                true,
            );
            batch.text(
                atlas,
                "Its folder moves to recovery, not permanent deletion.",
                x + 28.0,
                y + 98.0,
                rgba(180, 180, 185, 255),
                true,
            );
            button(batch, atlas, self.cursor, [x + 28.0, y + 120.0, 90.0, 20.0], "Delete", true);
            button(batch, atlas, self.cursor, [x + 124.0, y + 120.0, 90.0, 20.0], "Cancel", true);
        }

        // Keep the compiler honest that this screen is attached to this root;
        // click handling re-lists rather than retaining stale paths.
        let _ = root;
    }

    /// Pause-screen click interception. Returns true when Android management
    /// consumed the click so vanilla pause buttons must not also fire.
    pub(super) fn android_pause_click(&mut self) -> bool {
        let Some(root) = root() else { return false };
        let all = instances::list(&root);
        let batch = HudBatch::new(self.touch_pixels_for_manager());
        let h = batch.height;
        let name = current_name(&all);
        let chip_w = (HudBatch::text_width(&self.gui.atlas, &format!("Instance: {name}")) + 20.0)
            .clamp(120.0, 230.0);
        let chip = [8.0, h - 30.0, chip_w, 20.0];

        if !lock_state().open {
            if hovered(self.cursor, chip) {
                let mut state = lock_state();
                state.open = true;
                state.selected = Some(instances::current_id());
                state.status = None;
                self.audio.play_button_click();
                return true;
            }
            return false;
        }
        self.android_instance_panel_click(&root, &all, (batch.width, batch.height))
    }

    /// Title-screen variant of [`Self::android_pause_click`].
    pub(super) fn android_title_click(&mut self) -> bool {
        let Some(root) = root() else { return false };
        let all = instances::list(&root);
        let batch = HudBatch::new(self.touch_pixels_for_manager());
        let name = current_name(&all);
        let label = format!("{name} ▾");
        let width = (HudBatch::text_width(&self.gui.atlas, &label) + 20.0).clamp(92.0, 220.0);
        let chip = [8.0, 14.0, width, 20.0];
        if !lock_state().open {
            if hovered(self.cursor, chip) {
                let mut state = lock_state();
                state.open = true;
                state.selected = Some(instances::current_id());
                state.status = None;
                self.audio.play_button_click();
                return true;
            }
            return false;
        }
        self.android_instance_panel_click(&root, &all, (batch.width, batch.height))
    }

    fn android_instance_panel_click(
        &mut self,
        root: &std::path::Path,
        all: &[Instance],
        (w, h): (f32, f32),
    ) -> bool {
        let panel_w = PANEL_W.min(w - 16.0).max(220.0);
        let panel_h = (248.0f32).min(h - 16.0).max(190.0);
        let x = ((w - panel_w) / 2.0).floor();
        let y = ((h - panel_h) / 2.0).floor();
        let mut state = lock_state();

        if state.edit.is_some() {
            return true;
        }
        if let Some(id) = state.confirm_delete.clone() {
            if hovered(self.cursor, [x + 28.0, y + 120.0, 90.0, 20.0]) {
                state.confirm_delete = None;
                match instances::delete(root, &id) {
                    Ok(instance) => {
                        state.selected = Some(instances::current_id());
                        state.status = Some(format!("{} moved to recovery", instance.name));
                    }
                    Err(error) => state.status = Some(error),
                }
                self.audio.play_button_click();
            } else if hovered(self.cursor, [x + 124.0, y + 120.0, 90.0, 20.0]) {
                state.confirm_delete = None;
                self.audio.play_button_click();
            }
            return true;
        }

        let max_scroll = all.len().saturating_sub(VISIBLE_ROWS);
        state.scroll = state.scroll.min(max_scroll);
        let list_top = y + 39.0;
        for (visible, instance) in all
            .iter()
            .skip(state.scroll)
            .take(VISIBLE_ROWS)
            .enumerate()
        {
            if hovered(
                self.cursor,
                [x + 8.0, list_top + visible as f32 * ROW_H, panel_w - 16.0, 20.0],
            ) {
                state.selected = Some(instance.id.clone());
                state.status = None;
                self.audio.play_button_click();
                return true;
            }
        }

        let actions_y = y + panel_h - 58.0;
        let gap = 4.0;
        let bw = (panel_w - 20.0 - gap * 2.0) / 3.0;
        let action = |col: usize, row: usize| {
            [
                x + 10.0 + col as f32 * (bw + gap),
                actions_y + row as f32 * 24.0,
                bw,
                20.0,
            ]
        };

        if hovered(self.cursor, action(0, 0)) {
            let selected = state.selected.clone().unwrap_or_else(instances::current_id);
            if selected != instances::current_id() {
                match instances::select(root, &selected) {
                    Ok(instance) => {
                        state.status = Some(format!("Switching to {}…", instance.name));
                        drop(state);
                        self.audio.play_button_click();
                        if let Err(error) = crate::android::recreate_activity() {
                            lock_state().status = Some(format!("Selected; reopen Horizon ({error})"));
                        }
                        return true;
                    }
                    Err(error) => state.status = Some(error),
                }
            }
            self.audio.play_button_click();
            return true;
        }
        if hovered(self.cursor, action(1, 0)) {
            state.edit = Some(Edit {
                kind: EditKind::New,
                text: "New instance".into(),
            });
            state.status = None;
            drop(state);
            self.android_manager_keyboard(true);
            self.audio.play_button_click();
            return true;
        }
        if hovered(self.cursor, action(2, 0)) {
            let selected = state.selected.clone().unwrap_or_else(instances::current_id);
            if let Ok(instance) = instances::find(root, &selected) {
                let name = unique_copy_name(all, &instance.name);
                match instances::create(root, &name, Some(&instance.id)) {
                    Ok(copy) => {
                        state.selected = Some(copy.id);
                        state.status = Some(format!("Created {}", copy.name));
                    }
                    Err(error) => state.status = Some(error),
                }
            }
            self.audio.play_button_click();
            return true;
        }
        if hovered(self.cursor, action(0, 1)) {
            let selected = state.selected.clone().unwrap_or_else(instances::current_id);
            if let Ok(instance) = instances::find(root, &selected) {
                state.edit = Some(Edit {
                    kind: EditKind::Rename,
                    text: instance.name,
                });
                state.status = None;
                drop(state);
                self.android_manager_keyboard(true);
            }
            self.audio.play_button_click();
            return true;
        }
        if hovered(self.cursor, action(1, 1)) {
            let selected = state.selected.clone().unwrap_or_else(instances::current_id);
            if all
                .iter()
                .find(|instance| instance.id == selected)
                .is_some_and(|instance| instance.kind != Kind::Default)
            {
                state.confirm_delete = Some(selected);
            }
            self.audio.play_button_click();
            return true;
        }
        if hovered(self.cursor, action(2, 1)) {
            state.open = false;
            state.edit = None;
            state.confirm_delete = None;
            state.status = None;
            drop(state);
            self.android_manager_keyboard(false);
            self.audio.play_button_click();
            return true;
        }

        // Tap above/below the list to page it; this makes large collections
        // usable without stealing the game's mouse-wheel semantics.
        if all.len() > VISIBLE_ROWS && self.cursor.0 >= x && self.cursor.0 <= x + panel_w {
            if self.cursor.1 < list_top && state.scroll > 0 {
                state.scroll = state.scroll.saturating_sub(VISIBLE_ROWS);
                return true;
            }
            let list_bottom = list_top + VISIBLE_ROWS as f32 * ROW_H;
            if self.cursor.1 > list_bottom && self.cursor.1 < actions_y && state.scroll < max_scroll {
                state.scroll = (state.scroll + VISIBLE_ROWS).min(max_scroll);
                return true;
            }
        }

        // The open panel is modal even when the pointer misses a control.
        true
    }

    /// Consumes Android keyboard input only while the instance name editor is
    /// active. The normal chat/text paths remain untouched otherwise.
    pub(super) fn android_manager_key(&mut self, event: &KeyEvent) -> bool {
        let mut state = lock_state();
        let Some(edit) = state.edit.as_mut() else { return false };
        if event.state != ElementState::Pressed {
            return true;
        }

        match event.physical_key {
            PhysicalKey::Code(KeyCode::Escape) => {
                state.edit = None;
                drop(state);
                self.android_manager_keyboard(false);
                return true;
            }
            PhysicalKey::Code(KeyCode::Backspace) => {
                edit.text.pop();
                return true;
            }
            PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
                let submitted = edit.text.trim().to_owned();
                let kind = edit.kind;
                state.edit = None;
                if submitted.is_empty() {
                    state.status = Some("An instance needs a name.".into());
                    drop(state);
                    self.android_manager_keyboard(false);
                    return true;
                }
                let Some(root) = root() else {
                    state.status = Some("Android instance root is unavailable".into());
                    drop(state);
                    self.android_manager_keyboard(false);
                    return true;
                };
                let result = match kind {
                    EditKind::New => instances::create(&root, &submitted, None),
                    EditKind::Rename => {
                        let selected = state.selected.clone().unwrap_or_else(instances::current_id);
                        instances::rename(&root, &selected, &submitted)
                    }
                };
                match result {
                    Ok(instance) => {
                        state.selected = Some(instance.id.clone());
                        state.status = Some(match kind {
                            EditKind::New => format!("Created {}", instance.name),
                            EditKind::Rename => format!("Renamed to {}", instance.name),
                        });
                    }
                    Err(error) => state.status = Some(error),
                }
                drop(state);
                self.android_manager_keyboard(false);
                return true;
            }
            _ => {}
        }

        if let Some(text) = event.text.as_deref() {
            for ch in text.chars().filter(|ch| !ch.is_control()) {
                if edit.text.chars().count() >= 48 {
                    break;
                }
                edit.text.push(ch);
            }
        }
        true
    }

    fn android_manager_keyboard(&self, show: bool) {
        if let Some(window) = &self.window {
            window.set_ime_allowed(show);
        }
    }

    fn touch_pixels_for_manager(&self) -> [u32; 2] {
        self.renderer.as_ref().map_or_else(
            || {
                self.window
                    .as_ref()
                    .map(|window| window.inner_size().into())
                    .unwrap_or([1, 1])
            },
            |renderer| [renderer.config.width, renderer.config.height],
        )
    }
}

fn hovered(cursor: (f32, f32), [x, y, w, h]: [f32; 4]) -> bool {
    cursor.0 >= x && cursor.0 < x + w && cursor.1 >= y && cursor.1 < y + h
}

fn unique_copy_name(all: &[Instance], name: &str) -> String {
    for n in 1.. {
        let candidate = if n == 1 {
            format!("{name} Copy")
        } else {
            format!("{name} Copy {n}")
        };
        if !all
            .iter()
            .any(|instance| instance.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
    }
    unreachable!()
}
