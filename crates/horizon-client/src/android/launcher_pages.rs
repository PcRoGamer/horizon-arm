use super::*;
use horizon_client::{osk::Output, ui_theme::color};
use winit::{
    event::{KeyEvent, Touch},
    keyboard::{KeyCode, PhysicalKey},
};

#[derive(Clone, Copy)]
pub(super) enum Action {
    ToggleNative(usize),
    PackProfile,
    Page(Page),
    Play,
    Search,
    Field(&'static str),
    Install(usize),
    Remove(usize),
    Toggle(usize),
    Next,
    Previous,
    Profile,
    Logs,
    Renderer,
    ChooseRenderer(crate::catalog::RendererMode),
    ChooseProfile(u32),
    ChoosePack(usize),
    Dismiss,
}

pub(super) fn button(
    ui: &mut Ui,
    targets: &mut Vec<([f32; 4], Action)>,
    rect: [f32; 4],
    label: &str,
    action: Action,
    accent: bool,
) {
    let [x, y, w, h] = rect;
    ui.rounded(
        x,
        y,
        w,
        h,
        2.0,
        if accent { color::ACCENT } else { color::RAISED },
    );
    ui.text(
        Face::Medium,
        16.0,
        x + 12.0,
        y + 11.0,
        if accent {
            color::ON_ACCENT
        } else {
            color::TEXT
        },
        label,
    );
    targets.push((rect, action));
}
fn toggle(
    ui: &mut Ui,
    targets: &mut Vec<([f32; 4], Action)>,
    rect: [f32; 4],
    on: bool,
    action: Action,
) {
    let [x, y, w, h] = rect;
    ui.text(
        Face::Regular,
        14.0,
        x,
        y + 11.0,
        color::TEXT,
        if on { "On" } else { "Off" },
    );
    ui.rounded(
        x + w - 52.0,
        y + 8.0,
        48.0,
        24.0,
        12.0,
        if on { color::TEXT } else { color::HOVER },
    );
    ui.rounded(
        x + w - if on { 26.0 } else { 50.0 },
        y + 10.0,
        20.0,
        20.0,
        10.0,
        if on { color::BG } else { color::TEXT },
    );
    targets.push(([x, y, w, h], action));
}
impl Launcher {
    pub(super) fn draw_pages(&mut self) -> Result<()> {
        let (Some(r), Some(gui)) = (&mut self.renderer, &self.gui) else {
            return Ok(());
        };
        self.targets.clear();
        let mut b = HudBatch::new([r.config.width, r.config.height]);
        let mut ui = Ui::new(&gui.atlas, &mut b, (0.0, 0.0));
        let (w, h) = (ui.w, ui.h);
        let scale = ui.density / ui.batch.scale;
        ui.batch
            .fill(&gui.atlas, 0.0, 0.0, w * scale, h * scale, color::BG);
        ui.text(
            Face::Medium,
            32.0,
            32.0,
            28.0,
            color::TEXT,
            "Horizon Android",
        );
        ui.text(
            Face::Regular,
            14.0,
            w - 320.0,
            38.0,
            color::MUTED,
            "26.3 / Rust / Vulkan",
        );
        for (i, page) in Page::ALL.iter().enumerate() {
            let x = 32.0 + i as f32 * ((w - 64.0) / 6.0);
            button(
                &mut ui,
                &mut self.targets,
                [x, 88.0, (w - 76.0) / 6.0, 42.0],
                page.name(),
                Action::Page(*page),
                *page == self.catalog.page,
            );
        }
        let page = self.catalog.page;
        if page == Page::Play {
            ui.text(
                Face::Medium,
                28.0,
                32.0,
                165.0,
                color::TEXT,
                "Minecraft 26.3",
            );
            ui.text(
                Face::Regular,
                18.0,
                32.0,
                210.0,
                color::MUTED,
                &format!(
                    "Profile {} / {} / native WebAssembly mods",
                    self.catalog.config.profile,
                    self.catalog.config.renderer.label()
                ),
            );
            let text = if self.worker.is_some() {
                format!(
                    "Mojang download: {} / {} MiB",
                    self.progress.load(Ordering::Relaxed) / 1048576,
                    self.total.load(Ordering::Relaxed) / 1048576
                )
            } else if horizon_client::official_install::ready(&self.home) {
                "Installed. Ready to play offline.".into()
            } else {
                "Download game resources and sound directly from Mojang (HTTPS).".into()
            };
            ui.text(Face::Regular, 18.0, 32.0, 262.0, color::MUTED, &text);
            ui.text(
                Face::Regular,
                16.0,
                32.0,
                310.0,
                color::MUTED,
                "Install additions in Mods / Packs. Change player name and server in Settings.",
            );
            button(
                &mut ui,
                &mut self.targets,
                [32.0, 360.0, w - 64.0, 46.0],
                &format!(
                    "Modpack profile: {}  v",
                    self.catalog.config.pack.as_deref().unwrap_or("None")
                ),
                Action::PackProfile,
                false,
            );
            button(
                &mut ui,
                &mut self.targets,
                [32.0, h - 100.0, 320.0, 60.0],
                if self.worker.is_some() {
                    "Downloading..."
                } else if horizon_client::official_install::ready(&self.home) {
                    "Play"
                } else {
                    "Download 26.3"
                },
                Action::Play,
                true,
            );
        } else if page.is_catalog() {
            button(
                &mut ui,
                &mut self.targets,
                [32.0, 150.0, w - 240.0, 44.0],
                &format!("Search: {}", self.catalog.query),
                Action::Field("search"),
                false,
            );
            button(
                &mut ui,
                &mut self.targets,
                [w - 190.0, 150.0, 158.0, 44.0],
                "Search",
                Action::Search,
                true,
            );
            ui.text(Face::Regular,14.0,32.0,206.0,color::MUTED,if page==Page::Mods {"Search Java content mods, or paste an HTTPS .hmod URL. Java behavior cannot run."}else if page==Page::Modpacks {"Imports and ports data-driven content. Unsupported Java behavior is reported."}else{"Resource packs download with checksum verification. Enable them in Installed."});
            let top = 242.0;
            let bottom = h - 112.0;
            self.catalog.scroll = self.catalog.scroll.clamp(
                0.0,
                (self.catalog.results.len() as f32 * 78.0 - (bottom - top)).max(0.0),
            );
            for (i, p) in self.catalog.results.iter().enumerate() {
                let y = top + i as f32 * 78.0 - self.catalog.scroll;
                if y < top || y + 70.0 > bottom {
                    continue;
                }
                ui.text(
                    Face::Medium,
                    19.0,
                    32.0,
                    y,
                    color::TEXT,
                    &truncate(&p.title, 70),
                );
                ui.text(
                    Face::Regular,
                    14.0,
                    32.0,
                    y + 29.0,
                    color::MUTED,
                    &truncate(&p.summary, 100),
                );
                button(
                    &mut ui,
                    &mut self.targets,
                    [w - 150.0, y, 118.0, 42.0],
                    "Install",
                    Action::Install(i),
                    false,
                );
            }
            button(
                &mut ui,
                &mut self.targets,
                [32.0, h - 100.0, 120.0, 40.0],
                "Previous",
                Action::Previous,
                false,
            );
            button(
                &mut ui,
                &mut self.targets,
                [166.0, h - 100.0, 120.0, 40.0],
                "Next",
                Action::Next,
                false,
            );
        } else if page == Page::Installed {
            let top = 160.0;
            let bottom = h - 100.0;
            self.catalog.scroll = self.catalog.scroll.clamp(
                0.0,
                ((self.catalog.installed.entries.len() + self.catalog.native.len()) as f32 * 72.0
                    - (bottom - top))
                    .max(0.0),
            );
            if self.catalog.installed.entries.is_empty() && self.catalog.native.is_empty() {
                ui.text(
                    Face::Regular,
                    20.0,
                    32.0,
                    180.0,
                    color::MUTED,
                    "No browser additions in this profile. Install them from Mods or Packs.",
                );
            }
            for (i, e) in self.catalog.installed.entries.iter().enumerate() {
                let y = top + i as f32 * 72.0 - self.catalog.scroll;
                if y < top || y + 64.0 > bottom {
                    continue;
                }
                ui.text(
                    Face::Medium,
                    19.0,
                    32.0,
                    y,
                    color::TEXT,
                    &truncate(&e.title, 65),
                );
                ui.text(
                    Face::Regular,
                    14.0,
                    32.0,
                    y + 28.0,
                    color::MUTED,
                    &format!(
                        "{} / {}",
                        e.number,
                        e.verdict.as_deref().unwrap_or("Resource pack")
                    ),
                );
                toggle(
                    &mut ui,
                    &mut self.targets,
                    [w - 300.0, y, 120.0, 40.0],
                    self.catalog.enabled.get(i).copied().unwrap_or(false),
                    Action::Toggle(i),
                );
                button(
                    &mut ui,
                    &mut self.targets,
                    [w - 165.0, y, 133.0, 40.0],
                    "Remove",
                    Action::Remove(i),
                    false,
                );
            }
            for (i, (_, name, on)) in self.catalog.native.iter().enumerate() {
                let y = top + (i + self.catalog.installed.entries.len()) as f32 * 72.0
                    - self.catalog.scroll;
                if y < top || y + 64.0 > bottom {
                    continue;
                }
                ui.text(
                    Face::Medium,
                    19.0,
                    32.0,
                    y,
                    color::TEXT,
                    &truncate(name, 65),
                );
                ui.text(
                    Face::Regular,
                    14.0,
                    32.0,
                    y + 28.0,
                    color::MUTED,
                    "Native Horizon WebAssembly mod / sandboxed",
                );
                toggle(
                    &mut ui,
                    &mut self.targets,
                    [w - 165.0, y, 133.0, 40.0],
                    *on,
                    Action::ToggleNative(i),
                );
            }
        } else {
            let c = &self.catalog.config;
            ui.text(
                Face::Medium,
                24.0,
                32.0,
                156.0,
                color::TEXT,
                "Launch settings",
            );
            for (i, (title, value, action)) in [
                ("Rendering", c.renderer.label().to_owned(), Action::Renderer),
                ("Profile", format!("Profile {}", c.profile), Action::Profile),
                (
                    "Player name",
                    if c.name.is_empty() {
                        "Tap to enter".into()
                    } else {
                        c.name.clone()
                    },
                    Action::Field("name"),
                ),
                (
                    "Server",
                    if c.server.is_empty() {
                        "Singleplayer / tap to enter server".into()
                    } else {
                        c.server.clone()
                    },
                    Action::Field("server"),
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let y = 210.0 + i as f32 * 80.0;
                ui.text(Face::Regular, 16.0, 32.0, y + 13.0, color::MUTED, title);
                button(
                    &mut ui,
                    &mut self.targets,
                    [w * 0.36, y, w * 0.64 - 32.0, 46.0],
                    &format!(
                        "{} {}",
                        value,
                        if matches!(action, Action::Renderer | Action::Profile) {
                            "v"
                        } else {
                            ""
                        }
                    ),
                    action,
                    false,
                );
                ui.rounded(32.0, y + 64.0, w - 64.0, 1.0, 0.0, color::BORDER);
            }
            ui.text(
                Face::Regular,
                15.0,
                32.0,
                530.0,
                color::MUTED,
                "Resolution, chunks and FPS are controlled in Minecraft's Video Settings.",
            );
            ui.text(
                Face::Regular,
                15.0,
                32.0,
                554.0,
                color::MUTED,
                "The launcher preserves the game's settings. Rendering changes apply next launch.",
            );
            button(
                &mut ui,
                &mut self.targets,
                [32.0, h - 114.0, 270.0, 42.0],
                "Startup diagnostics",
                Action::Logs,
                false,
            );
        }
        let status = self.error.as_deref().unwrap_or(&self.catalog.status);
        for (i, line) in status
            .chars()
            .collect::<Vec<_>>()
            .chunks(110)
            .take(2)
            .enumerate()
        {
            ui.text(
                Face::Regular,
                14.0,
                32.0,
                h - 47.0 + i as f32 * 18.0,
                color::MUTED,
                &line.iter().collect::<String>(),
            );
        }
        if let Some(chooser) = &mut self.chooser {
            chooser.draw(&mut ui, &mut self.targets);
        }
        ui.finish();

        r.render(&game::loading_frame(&b))?;
        Ok(())
    }
    pub(super) fn activate(&mut self, events: &ActiveEventLoop) -> Result<()> {
        let Some(r) = &self.renderer else {
            return Ok(());
        };
        let mut b = HudBatch::new([r.config.width, r.config.height]);
        let ui = Ui::new(&self.gui.as_ref().unwrap().atlas, &mut b, (0.0, 0.0));
        let p = [
            self.cursor.0 as f32 / ui.density,
            self.cursor.1 as f32 / ui.density,
        ];
        let action = self
            .targets
            .iter()
            .find(|([x, y, w, h], _)| {
                (*x..*x + *w).contains(&p[0]) && (*y..*y + *h).contains(&p[1])
            })
            .map(|(_, a)| *a);
        if self.catalog.busy() {
            return Ok(());
        }
        match action {
            Some(Action::ToggleNative(i)) => self.catalog.toggle_native(&self.home, i),
            Some(Action::PackProfile) => self.open_chooser(chooser::Kind::Pack),
            Some(Action::Page(page)) => {
                self.catalog.config.save(&self.home)?;
                self.catalog.editing = None;
                crate::platform::hide();
                self.catalog.page = page;
                self.catalog.scroll = 0.0;
                self.error = None;
                self.catalog.refresh(&self.home);
                if page.is_catalog() {
                    self.catalog.offset = 0;
                    self.catalog.search(&self.home);
                }
            }
            Some(Action::Play) => self.action(events)?,
            Some(Action::Search) => {
                crate::platform::hide();
                self.catalog.offset = 0;
                self.catalog.search(&self.home);
            }
            Some(Action::Field(name)) => {
                self.catalog.editing = Some(name);
                let initial = match name {
                    "name" => &self.catalog.config.name,
                    "server" => &self.catalog.config.server,
                    _ => &self.catalog.query,
                };
                crate::platform::show(initial);
            }
            Some(Action::Install(i)) => self.catalog.install(&self.home, i),
            Some(Action::Remove(i)) => self.catalog.remove(&self.home, i),
            Some(Action::Toggle(i)) => self.catalog.toggle(&self.home, i),
            Some(Action::Next) => {
                self.catalog.offset += 12;
                self.catalog.search(&self.home);
            }
            Some(Action::Previous) => {
                self.catalog.offset = self.catalog.offset.saturating_sub(12);
                self.catalog.search(&self.home);
            }
            Some(Action::Profile) => self.open_chooser(chooser::Kind::Profile),
            Some(Action::Renderer) => self.open_chooser(chooser::Kind::Renderer),
            Some(Action::ChooseRenderer(mode)) => {
                self.catalog.config.renderer = mode;
                self.catalog.config.save(&self.home)?;
                self.chooser = None;
            }
            Some(Action::ChooseProfile(profile)) => {
                std::fs::create_dir_all(if profile == 0 {
                    self.home.clone()
                } else {
                    self.home
                        .join("profiles")
                        .join(format!("profile-{profile}"))
                })?;
                self.catalog.config.profile = profile;
                self.catalog.config.pack = None;
                self.catalog.config.save(&self.home)?;
                self.catalog.refresh(&self.home);
                self.chooser = None;
            }
            Some(Action::ChoosePack(i)) => {
                let pack = if i == 0 {
                    None
                } else {
                    self.catalog.pack_ids(&self.home).get(i - 1).cloned()
                };
                self.catalog.select_pack(&self.home, pack)?;
                self.chooser = None;
            }
            Some(Action::Dismiss) => self.chooser = None,
            Some(Action::Logs) => {
                self.catalog.status = std::fs::read_to_string(self.home.join("android-error.txt"))
                    .unwrap_or_else(|_| {
                        "No startup error. In game: F3 shows device, terrain and frame diagnostics."
                            .into()
                    });
            }
            None => {}
        }
        Ok(())
    }
    pub(super) fn touch_event(&mut self, events: &ActiveEventLoop, t: Touch) -> Result<()> {
        self.cursor = (t.location.x, t.location.y);
        match t.phase {
            TouchPhase::Started => {
                self.drag = Some((t.location.x, t.location.y, false));
            }
            TouchPhase::Moved => {
                if let Some((x, y, moved)) = &mut self.drag {
                    if (t.location.x - *x).hypot(t.location.y - *y) > 12.0 {
                        *moved = true;
                    }
                    if *moved {
                        if let Some(chooser) = &mut self.chooser {
                            chooser.scroll = (chooser.scroll + (*y - t.location.y) as f32).max(0.0);
                        } else if t.location.y > 230.0 {
                            self.catalog.scroll =
                                (self.catalog.scroll + (*y - t.location.y) as f32).max(0.0);
                        }
                    }
                    *y = t.location.y;
                }
            }
            TouchPhase::Ended => {
                if self.drag.take().is_some_and(|(_, _, moved)| !moved) {
                    self.activate(events)?;
                }
            }
            TouchPhase::Cancelled => {
                self.drag = None;
            }
        }
        Ok(())
    }
    pub(super) fn type_key(&mut self, key: KeyEvent) {
        if key.physical_key == PhysicalKey::Code(KeyCode::Escape)
            || key.logical_key
                == winit::keyboard::Key::Named(winit::keyboard::NamedKey::BrowserBack)
        {
            if self.chooser.take().is_some() {
                return;
            }
            crate::platform::hide();
            self.catalog.editing = None;
            let _ = self.catalog.config.save(&self.home);
            return;
        }
        if self.catalog.editing.is_none() {
            return;
        }
        let out = match key.physical_key {
            PhysicalKey::Code(KeyCode::Backspace) => Some(Output::Backspace),
            PhysicalKey::Code(KeyCode::Enter) => {
                crate::platform::hide();
                Some(Output::Enter)
            }
            _ => key.text.map(|s| Output::Text(s.into())),
        };
        if let Some(out) = out {
            self.catalog.edit(out, &self.home);
        }
    }
}
fn truncate(s: &str, max: usize) -> String {
    let mut value: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        value.push_str("...");
    }
    value
}
