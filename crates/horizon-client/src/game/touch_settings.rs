//! Bedrock-style touch settings surfaced inside Horizon's pause menu.
//!
//! This is intentionally in-game rather than launcher UI. Changes apply to the
//! current Android touch HUD immediately and are persisted in the instance's
//! `horizon-touch.txt` for the next launch.

use super::Game;
use crate::{
    hud::{HudBatch, rgba},
    text::Text,
    ui,
};
use std::sync::{Mutex, OnceLock};

const PANEL_W: f32 = 332.0;
const PANEL_H: f32 = 246.0;
const ROW_H: f32 = 27.0;

#[derive(Default)]
struct PanelState {
    open: bool,
    status: Option<String>,
}

static STATE: OnceLock<Mutex<PanelState>> = OnceLock::new();

fn state() -> &'static Mutex<PanelState> {
    STATE.get_or_init(|| Mutex::new(PanelState::default()))
}

fn lock_state() -> std::sync::MutexGuard<'static, PanelState> {
    state().lock().unwrap_or_else(|e| e.into_inner())
}

fn hovered(cursor: (f32, f32), [x, y, w, h]: [f32; 4]) -> bool {
    cursor.0 >= x && cursor.0 < x + w && cursor.1 >= y && cursor.1 < y + h
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
    let hot = enabled && ui::hovered(cursor, x, y, w, h);
    ui::button(
        batch,
        atlas,
        x,
        y,
        w,
        h,
        &Text::literal(label.into()),
        enabled,
        hot,
    );
    hot
}

impl Game<'_> {
    /// Compact entry point on the pause screen plus the modal settings panel.
    pub(super) fn draw_android_touch_settings_entry(&mut self, batch: &mut HudBatch) {
        let atlas = &self.gui.atlas;
        let h = batch.height;
        let chip = [8.0, h - 54.0, 130.0, 20.0];
        button(batch, atlas, self.cursor, chip, "Touch Controls", true);

        if !lock_state().open {
            return;
        }
        self.draw_android_touch_settings(batch);
    }

    fn draw_android_touch_settings(&mut self, batch: &mut HudBatch) {
        let atlas = &self.gui.atlas;
        let w = batch.width;
        let h = batch.height;
        let panel_w = PANEL_W.min(w - 16.0).max(250.0);
        let panel_h = PANEL_H.min(h - 16.0).max(210.0);
        let x = ((w - panel_w) * 0.5).floor();
        let y = ((h - panel_h) * 0.5).floor();

        batch.fill(atlas, x - 2.0, y - 2.0, panel_w + 4.0, panel_h + 4.0, rgba(0, 0, 0, 214));
        batch.fill(atlas, x, y, panel_w, panel_h, rgba(24, 25, 29, 248));
        batch.text(atlas, "Touch Controls", x + 10.0, y + 9.0, u32::MAX, true);
        batch.text(
            atlas,
            "Bedrock-style mobile controls · applies immediately",
            x + 10.0,
            y + 21.0,
            rgba(166, 168, 174, 255),
            true,
        );

        let rows = [
            ("Control mode", self.touch_control_mode_name()),
            ("Joystick", self.touch_joystick_mode_name()),
            ("Action buttons", self.touch_action_buttons_name()),
            ("Handedness", self.touch_handedness_name()),
            ("Joystick visibility", self.touch_joystick_visibility_name()),
        ];
        let list_y = y + 43.0;
        for (i, (label, value)) in rows.into_iter().enumerate() {
            let ry = list_y + i as f32 * ROW_H;
            if i % 2 == 0 {
                batch.fill(atlas, x + 8.0, ry - 2.0, panel_w - 16.0, ROW_H, rgba(38, 39, 45, 128));
            }
            batch.text(atlas, label, x + 14.0, ry + 6.0, rgba(224, 224, 228, 255), true);
            let value_w = (HudBatch::text_width(atlas, value) + 18.0).clamp(94.0, 168.0);
            button(
                batch,
                atlas,
                self.cursor,
                [x + panel_w - value_w - 12.0, ry, value_w, 20.0],
                value,
                true,
            );
        }

        let actions_y = y + panel_h - 51.0;
        let bw = (panel_w - 28.0) / 3.0;
        button(batch, atlas, self.cursor, [x + 8.0, actions_y, bw, 20.0], "Reset", true);
        button(
            batch,
            atlas,
            self.cursor,
            [x + 14.0 + bw, actions_y, bw, 20.0],
            "Customize",
            false,
        );
        button(
            batch,
            atlas,
            self.cursor,
            [x + 20.0 + bw * 2.0, actions_y, bw, 20.0],
            "Done",
            true,
        );
        batch.text(
            atlas,
            "Customize layout is the next parity layer; disabled until drag/resize is complete.",
            x + 10.0,
            actions_y + 25.0,
            rgba(145, 147, 153, 255),
            true,
        );

        if let Some(status) = &lock_state().status {
            let text = if status.chars().count() > 50 {
                status.chars().take(47).collect::<String>() + "…"
            } else {
                status.clone()
            };
            batch.text(atlas, &text, x + 10.0, y + panel_h - 12.0, rgba(174, 220, 174, 255), true);
        }
    }

    /// First refusal for pause-screen clicks. Returns false when the touch panel
    /// is closed and its entry chip was not pressed, allowing instance/vanilla UI.
    pub(super) fn android_touch_settings_click(&mut self) -> bool {
        let [width, height] = self.renderer.as_ref().map_or_else(
            || self.window.as_ref().map(|w| w.inner_size().into()).unwrap_or([1, 1]),
            |r| [r.config.width, r.config.height],
        );
        let batch = HudBatch::new([width, height]);
        let chip = [8.0, batch.height - 54.0, 130.0, 20.0];

        if !lock_state().open {
            if !hovered(self.cursor, chip) {
                return false;
            }
            let mut state = lock_state();
            state.open = true;
            state.status = None;
            self.audio.play_button_click();
            return true;
        }

        let w = batch.width;
        let h = batch.height;
        let panel_w = PANEL_W.min(w - 16.0).max(250.0);
        let panel_h = PANEL_H.min(h - 16.0).max(210.0);
        let x = ((w - panel_w) * 0.5).floor();
        let y = ((h - panel_h) * 0.5).floor();
        let list_y = y + 43.0;

        for i in 0..5 {
            let rect = [x + 8.0, list_y + i as f32 * ROW_H - 2.0, panel_w - 16.0, ROW_H];
            if hovered(self.cursor, rect) {
                let result = match i {
                    0 => self.touch_cycle_control_mode(),
                    1 => self.touch_toggle_static_joystick(),
                    2 => self.touch_toggle_action_buttons(),
                    3 => self.touch_toggle_handedness(),
                    4 => self.touch_cycle_joystick_visibility(),
                    _ => unreachable!(),
                };
                let mut state = lock_state();
                state.status = Some(result.unwrap_or_else(|error| error));
                self.audio.play_button_click();
                return true;
            }
        }

        let actions_y = y + panel_h - 51.0;
        let bw = (panel_w - 28.0) / 3.0;
        if hovered(self.cursor, [x + 8.0, actions_y, bw, 20.0]) {
            let result = self.touch_reset_settings();
            lock_state().status = Some(result.unwrap_or_else(|error| error));
            self.audio.play_button_click();
            return true;
        }
        if hovered(self.cursor, [x + 20.0 + bw * 2.0, actions_y, bw, 20.0]) {
            let mut state = lock_state();
            state.open = false;
            state.status = None;
            self.audio.play_button_click();
            return true;
        }

        // Modal while open; do not let the vanilla pause screen receive the tap.
        true
    }
}
