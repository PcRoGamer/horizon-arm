//! Explicit, scrollable selections. Values never cycle on repeated button taps.
use super::{
    Launcher,
    pages::{Action, button},
};
use crate::catalog::RendererMode;
use horizon_client::ui_theme::{Face, Ui, color};

pub(super) enum Kind {
    Renderer,
    Profile,
    Pack,
}
pub(super) struct Chooser {
    title: &'static str,
    help: &'static str,
    device: Option<String>,
    choices: Vec<(String, String, Action, bool)>,
    pub scroll: f32,
}
impl Launcher {
    pub(super) fn open_chooser(&mut self, kind: Kind) {
        crate::platform::hide();
        self.catalog.editing = None;
        let device = if matches!(kind, Kind::Renderer) {
            self.renderer.as_ref().map(|r| {
                format!(
                    "GPU: {} / {} {}",
                    r.info.name, r.info.driver, r.info.driver_info
                )
            })
        } else {
            None
        };
        let c = &self.catalog.config;
        let (title, help, choices) = match kind {
            Kind::Renderer => (
                "Rendering mode",
                "API: Vulkan. Uses the system GPU driver. Applies next launch.",
                vec![
                    (
                        "Vulkan / GPU batching".into(),
                        "Groups terrain draws and enables supported GPU culling.".into(),
                        Action::ChooseRenderer(RendererMode::Vulkan),
                        c.renderer == RendererMode::Vulkan,
                    ),
                    (
                        "Vulkan / compatibility".into(),
                        "Direct terrain draws for drivers with missing or transparent blocks."
                            .into(),
                        Action::ChooseRenderer(RendererMode::VulkanCompatibility),
                        c.renderer == RendererMode::VulkanCompatibility,
                    ),
                ],
            ),
            Kind::Profile => {
                let mut ids = vec![0, c.profile];
                ids.extend(
                    std::fs::read_dir(self.home.join("profiles"))
                        .into_iter()
                        .flatten()
                        .flatten()
                        .filter(|e| e.path().is_dir())
                        .filter_map(|e| {
                            e.file_name()
                                .to_str()?
                                .strip_prefix("profile-")?
                                .parse::<u32>()
                                .ok()
                        }),
                );
                ids.sort_unstable();
                ids.dedup();
                let next = ids.last().copied().unwrap_or(0).checked_add(1);
                let mut choices: Vec<_> = ids
                    .into_iter()
                    .map(|id| {
                        (
                            format!("Profile {id}"),
                            if id == 0 {
                                "Default profile".into()
                            } else {
                                "Separate saves, mods and packs".into()
                            },
                            Action::ChooseProfile(id),
                            id == c.profile,
                        )
                    })
                    .collect();
                if let Some(id) = next {
                    choices.push((
                        "Create profile".into(),
                        format!("Create Profile {id}"),
                        Action::ChooseProfile(id),
                        false,
                    ));
                }
                (
                    "Choose profile",
                    "Each profile keeps its own game settings and saves.",
                    choices,
                )
            }
            Kind::Pack => {
                let mut choices = vec![(
                    "None".into(),
                    "Use the selected profile without an imported modpack.".into(),
                    Action::ChoosePack(0),
                    c.pack.is_none(),
                )];
                choices.extend(
                    self.catalog
                        .pack_ids(&self.home)
                        .into_iter()
                        .enumerate()
                        .map(|(i, id)| {
                            let selected = c.pack.as_ref() == Some(&id);
                            (
                                id,
                                "Imported modpack".into(),
                                Action::ChoosePack(i + 1),
                                selected,
                            )
                        }),
                );
                (
                    "Choose modpack",
                    "Choose an imported pack, or install one from Modpacks.",
                    choices,
                )
            }
        };
        self.chooser = Some(Chooser {
            title,
            help,
            choices,
            device,
            scroll: 0.0,
        });
    }
}
impl Chooser {
    pub fn draw(&mut self, ui: &mut Ui, targets: &mut Vec<([f32; 4], Action)>) {
        let (w, h) = (ui.w, ui.h);
        targets.clear();
        ui.rounded(0.0, 0.0, w, h, 0.0, color::BG);
        ui.text(Face::Medium, 28.0, 32.0, 35.0, color::TEXT, self.title);
        ui.text(Face::Regular, 15.0, 32.0, 85.0, color::MUTED, self.help);
        if let Some(device) = &self.device {
            ui.text(Face::Regular, 13.0, 32.0, 111.0, color::MUTED, device);
        }
        let top = 137.0;
        let bottom = h - 100.0;
        self.scroll = self.scroll.clamp(
            0.0,
            (self.choices.len() as f32 * 84.0 - (bottom - top)).max(0.0),
        );
        for (i, (label, detail, action, selected)) in self.choices.iter().enumerate() {
            let y = top + i as f32 * 84.0 - self.scroll;
            if y < top || y + 76.0 > bottom {
                continue;
            }
            ui.rounded(34.0, y + 10.0, 22.0, 22.0, 11.0, color::TEXT);
            ui.rounded(36.0, y + 12.0, 18.0, 18.0, 9.0, color::BG);
            if *selected {
                ui.rounded(40.0, y + 16.0, 10.0, 10.0, 5.0, color::TEXT);
            }
            ui.text(Face::Medium, 19.0, 80.0, y + 8.0, color::TEXT, label);
            ui.text(Face::Regular, 14.0, 80.0, y + 40.0, color::MUTED, detail);
            ui.rounded(32.0, y + 75.0, w - 64.0, 1.0, 0.0, color::BORDER);
            targets.push(([32.0, y, w - 64.0, 76.0], *action));
        }
        button(
            ui,
            targets,
            [32.0, h - 78.0, 160.0, 44.0],
            "Cancel",
            Action::Dismiss,
            false,
        );
    }
}
