use super::*;

pub struct Editor {
    pub selected: Control,
    original: Settings,
    drag: Option<(u64, Drag)>,
    error: Option<String>,
}
enum Drag {
    Control([f32; 2]),
    Slider(usize),
}
const OPTIONS: [&str; 4] = ["Split", "Left hand", "Auto sprint", "Swap jump/sneak"];
const SLIDERS: [&str; 3] = ["Size", "Opacity", "Sensitivity"];

impl TouchControls {
    pub fn open_editor(&mut self) {
        self.clear();
        self.editor = Some(Editor {
            selected: Control::Move,
            original: self.settings.clone(),
            drag: None,
            error: None,
        });
    }
    pub fn cancel_editor(&mut self) {
        if let Some(e) = self.editor.take() {
            self.settings = e.original;
        }
        self.clear();
    }
    /// The editor owns every finger: editing cannot move, mine or open menus.
    /// Returns true when Done/Cancel closed the editor.
    pub fn edit_touch(&mut self, id: u64, p: [f32; 2], started: bool, ended: bool) -> bool {
        let Some(mut e) = self.editor.take() else {
            return false;
        };
        if started {
            if layout::contains([0.32, 0.91, 0.11, 0.075], p) {
                match self.settings.save() {
                    Ok(()) => {
                        self.clear();
                        return true;
                    }
                    Err(error) => e.error = Some(error.to_string()),
                }
            } else if layout::contains([0.44, 0.91, 0.11, 0.075], p) {
                self.settings = e.original;
                self.clear();
                return true;
            } else if layout::contains([0.56, 0.91, 0.12, 0.075], p) {
                self.settings.layouts[self.settings.mode.index()] =
                    Settings::default().layouts[self.settings.mode.index()];
            } else if (0.12..0.20).contains(&p[1]) {
                let index = ((p[0] - 0.02) / 0.32).floor() as usize;
                if let Some(mode) = Mode::ALL.get(index) {
                    self.settings.mode = *mode;
                }
            } else if (0.22..0.29).contains(&p[1]) {
                match ((p[0] - 0.02) / 0.24).floor() as usize {
                    0 if self.settings.mode == Mode::Dpad => {
                        self.settings.split_controls = !self.settings.split_controls
                    }
                    1 => self.settings.left_handed = !self.settings.left_handed,
                    2 if self.settings.mode != Mode::Dpad => {
                        self.settings.sprint_with_joystick = !self.settings.sprint_with_joystick
                    }
                    3 if self.settings.mode == Mode::Dpad => {
                        self.settings.swap_jump_sneak = !self.settings.swap_jump_sneak
                    }
                    _ => {}
                }
            } else if (0.32..0.68).contains(&p[0]) && (0.68..0.89).contains(&p[1]) {
                let index = ((p[1] - 0.68) / 0.07).floor() as usize;
                e.drag = Some((id, Drag::Slider(index.min(2))));
            } else if let Some(c) = Control::ALL.into_iter().rev().find(|c| {
                self.visible(*c)
                    && layout::contains(self.settings.placement(*c).rect(self.aspect()), p)
            }) {
                e.selected = c;
                let center = self.settings.placement(c).center;
                e.drag = Some((id, Drag::Control([p[0] - center[0], p[1] - center[1]])));
            }
        }
        if let Some((owner, drag)) = &e.drag
            && *owner == id
        {
            let aspect = self.aspect();
            let slot = self.settings.slot(e.selected);
            let place = &mut self.settings.layouts[self.settings.mode.index()][slot];
            match drag {
                Drag::Control(offset) => {
                    let mut center = [p[0] - offset[0], p[1] - offset[1]];
                    if self.settings.left_handed {
                        center[0] = 1.0 - center[0];
                    }
                    place.center = [
                        center[0].clamp(place.size / aspect / 2.0, 1.0 - place.size / aspect / 2.0),
                        center[1].clamp(place.size / 2.0, 1.0 - place.size / 2.0),
                    ];
                }
                Drag::Slider(index) => {
                    let value = ((p[0] - 0.335) / 0.33).clamp(0.0, 1.0);
                    match index {
                        0 => place.size = 0.045 + value * 0.405,
                        1 => place.opacity = value,
                        _ => self.settings.sensitivity = 0.1 + value * 2.9,
                    }
                }
            }
            if ended {
                e.drag = None;
            }
        }
        self.editor = Some(e);
        false
    }
    pub(super) fn draw_editor(&self, b: &mut HudBatch, a: &HudAtlas) {
        let e = self.editor.as_ref().unwrap();
        let (w, h) = (b.width, b.height);
        let text = rgba(245, 245, 245, 255);
        for (i, mode) in Mode::ALL.into_iter().enumerate() {
            plate(
                b,
                a,
                [0.02 + i as f32 * 0.32, 0.12, 0.30, 0.08],
                mode.label(),
                self.settings.mode == mode,
            );
        }
        for (i, (name, on)) in OPTIONS
            .into_iter()
            .zip([
                self.settings.split_controls,
                self.settings.left_handed,
                self.settings.sprint_with_joystick,
                self.settings.swap_jump_sneak,
            ])
            .enumerate()
        {
            plate(
                b,
                a,
                [0.02 + i as f32 * 0.24, 0.22, 0.22, 0.07],
                &format!("{name}: {}", if on { "On" } else { "Off" }),
                on,
            );
        }
        let label = "Touch controls: tap a button, then drag it";
        b.text(
            a,
            label,
            (w - HudBatch::text_width(a, label)) / 2.0,
            h * 0.32,
            text,
            true,
        );
        b.fill(
            a,
            w * 0.31,
            h * 0.61,
            w * 0.38,
            h * 0.39,
            rgba(15, 15, 15, 235),
        );
        let p = self.settings.placement(e.selected);
        b.text(a, e.selected.name(), w * 0.33, h * 0.63, text, true);
        let values = [
            (p.size - 0.045) / 0.405,
            p.opacity,
            (self.settings.sensitivity - 0.1) / 2.9,
        ];
        for (i, name) in SLIDERS.into_iter().enumerate() {
            let y = 0.68 + i as f32 * 0.07;
            let percent = match i {
                0 => p.size * 100.0,
                1 => p.opacity * 100.0,
                _ => self.settings.sensitivity * 100.0,
            };
            b.text(
                a,
                &format!("{name} {:.0}%", percent),
                w * 0.325,
                h * (y + 0.004),
                text,
                true,
            );
            nine(
                b,
                a,
                "ui/slider_background",
                1.0,
                [w * 0.335, h * (y + 0.045), w * 0.33, h * 0.025],
                rgba(255, 255, 255, 255),
            );
            nine(
                b,
                a,
                "ui/slider_button_default",
                2.0,
                [
                    w * (0.335 + values[i] * 0.33) - 3.0,
                    h * (y + 0.032),
                    6.0,
                    h * 0.05,
                ],
                rgba(255, 255, 255, 255),
            );
        }
        for (rect, label) in [
            ([0.32, 0.91, 0.11, 0.075], "Done"),
            ([0.44, 0.91, 0.11, 0.075], "Cancel"),
            ([0.56, 0.91, 0.12, 0.075], "Reset"),
        ] {
            plate(b, a, rect, label, false);
        }
        if e.error.is_some() {
            b.text(
                a,
                "Save failed. Retry Done.",
                w * 0.33,
                h * 0.88,
                rgba(255, 120, 120, 255),
                true,
            );
        }
    }
}
fn plate(b: &mut HudBatch, a: &HudAtlas, [x, y, w, h]: [f32; 4], label: &str, on: bool) {
    let (bw, bh) = (b.width, b.height);
    nine(
        b,
        a,
        "ui/button_borderless_light",
        1.0,
        [x * bw, y * bh, w * bw, h * bh],
        if on {
            rgba(100, 210, 90, 255)
        } else {
            rgba(255, 255, 255, 255)
        },
    );
    if let Some((first, second)) = label.split_once(" & ") {
        b.text(
            a,
            &format!("{first} &"),
            x * bw + 3.0,
            y * bh + (h * bh - 16.0) / 2.0,
            rgba(35, 35, 35, 255),
            true,
        );
        b.text(
            a,
            second,
            x * bw + 3.0,
            y * bh + (h * bh - 16.0) / 2.0 + 8.0,
            rgba(35, 35, 35, 255),
            true,
        );
    } else {
        let mut display = label.to_owned();
        while HudBatch::text_width(a, &display) > w * bw - 6.0 && !display.is_empty() {
            display.pop();
        }
        b.text(
            a,
            &display,
            x * bw + 3.0,
            y * bh + (h * bh - 8.0) / 2.0,
            rgba(35, 35, 35, 255),
            true,
        );
    }
}
