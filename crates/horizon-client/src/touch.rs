//! Multitouch input using the engine's existing controller actions.
use crate::gamepad::{Button, PadState};
use crate::hud::{HudAtlas, HudBatch, rgba};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
mod editor;
pub mod layout;
pub use layout::{Control, Mode, Settings};

/// A finger ray through the perspective projection, without rotating the view.
pub fn screen_direction(
    forward: [f64; 3],
    up: [f64; 3],
    point: [f32; 2],
    aspect: f64,
    fov: f64,
) -> [f64; 3] {
    let right = [
        forward[1] * up[2] - forward[2] * up[1],
        forward[2] * up[0] - forward[0] * up[2],
        forward[0] * up[1] - forward[1] * up[0],
    ];
    let t = (fov.to_radians() / 2.0).tan();
    let x = (f64::from(point[0]) * 2.0 - 1.0) * aspect * t;
    let y = (1.0 - f64::from(point[1]) * 2.0) * t;
    let ray = std::array::from_fn::<_, 3, _>(|i| forward[i] + right[i] * x + up[i] * y);
    let length = ray.iter().map(|v| v * v).sum::<f64>().sqrt();
    ray.map(|v| v / length)
}

pub fn hotbar_slot(width: f32, height: f32, point: [f32; 2]) -> Option<usize> {
    let x = point[0] - (width / 2.0 - 90.0);
    (point[1] >= height - 22.0 && point[1] < height && (0.0..180.0).contains(&x))
        .then(|| (x / 20.0) as usize)
}
#[derive(Clone, Copy)]
enum Role {
    Move([f32; 2]),
    Look {
        start: [f32; 2],
        since: Instant,
        dragged: bool,
    },
    Ignore,
    Button(Button),
}
#[derive(Default)]
pub struct TouchControls {
    pub settings: Settings,
    pub editor: Option<editor::Editor>,
    fingers: BTreeMap<u64, (Role, [f32; 2])>,
    pending: PadState,
    pub aim: Option<[f32; 2]>,
    tap_aim: Option<[f32; 2]>,
    aspect: f32,
    loaded: bool,
    pub chat_requested: bool,
    pub settings_requested: bool,
    pub context_use: bool,
    pub vertical_movement: bool,
    pub swimming: bool,
    pub mounted: bool,
}
impl TouchControls {
    pub fn viewport(&mut self, width: u32, height: u32) {
        self.aspect = width.max(1) as f32 / height.max(1) as f32;
        if !self.loaded {
            self.settings = Settings::load();
            self.loaded = true;
        }
    }
    fn aspect(&self) -> f32 {
        if self.aspect > 0.0 {
            self.aspect
        } else {
            16.0 / 9.0
        }
    }
    pub fn ignore(&mut self, id: u64) {
        self.fingers.insert(id, (Role::Ignore, [0.0; 2]));
    }
    pub fn tap(&mut self, button: Button) {
        self.pending.set(button, true);
    }
    fn visible(&self, c: Control) -> bool {
        self.settings.visible(c) || (c == Control::Use && self.context_use)
    }
    fn button(c: Control) -> Option<Button> {
        Some(match c {
            Control::Sprint => Button::LeftStick,
            Control::Jump => Button::South,
            Control::Sneak => Button::RightStick,
            Control::Attack => Button::RightTrigger,
            Control::Use => Button::LeftTrigger,
            Control::Pause => Button::Start,
            Control::Inventory => Button::North,
            _ => return None,
        })
    }
    pub fn draw(&self, b: &mut HudBatch, a: &HudAtlas) {
        let (w, h) = (b.width, b.height);
        let state = self.state();
        for control in Control::ALL {
            if !self.visible(control) {
                continue;
            }
            let p = self.settings.placement(control);
            let size = p.size * h;
            let (x, y) = (p.center[0] * w - size / 2.0, p.center[1] * h - size / 2.0);
            let opacity = if self.editor.is_some() {
                p.opacity.max(0.25)
            } else {
                p.opacity
            };
            let color = rgba(255, 255, 255, (opacity * 255.0).round() as u8);
            let held = Self::button(control).is_some_and(|bt| {
                state.held(bt)
                    || (bt == Button::LeftTrigger && state.triggers[0] > 0.0)
                    || (bt == Button::RightTrigger && state.triggers[1] > 0.0)
            });
            if control == Control::Move {
                if self.settings.mode == Mode::Dpad {
                    let cell = size / 3.0;
                    for (dx, dy, name) in [
                        (0, -1, "up"),
                        (-1, 0, "left"),
                        (1, 0, "right"),
                        (0, 1, "down"),
                    ] {
                        let pressed = match (dx, dy) {
                            (0, -1) => state.left[1] > 0.2,
                            (0, 1) => state.left[1] < -0.2,
                            (-1, 0) => state.left[0] < -0.2,
                            _ => state.left[0] > 0.2,
                        };
                        sprite(
                            b,
                            a,
                            &format!("controls/{name}{}", if pressed { "_pressed" } else { "" }),
                            [
                                x + cell * (dx + 1) as f32,
                                y + cell * (dy + 1) as f32,
                                cell,
                                cell,
                            ],
                            color,
                        );
                    }
                    // Vanilla shows the diagonal keys while their adjoining arrow is held.
                    if state.left[1].abs() > 0.2 {
                        let (dy, prefix) = if state.left[1] > 0.0 {
                            (0, "up")
                        } else {
                            (2, "down")
                        };
                        for (dx, suffix) in [(0, "left"), (2, "right")] {
                            let pressed = if dx == 0 {
                                state.left[0] < -0.2
                            } else {
                                state.left[0] > 0.2
                            };
                            sprite(
                                b,
                                a,
                                &format!(
                                    "controls/{prefix}_{suffix}{}",
                                    if pressed { "_pressed" } else { "" }
                                ),
                                [x + cell * dx as f32, y + cell * dy as f32, cell, cell],
                                color,
                            );
                        }
                    }
                } else {
                    sprite(b, a, "ui/joystick_frame", [x, y, size, size], color);
                    sprite(
                        b,
                        a,
                        "ui/joystick_knob",
                        [
                            x + size * (0.33 + state.left[0] * 0.25),
                            y + size * (0.33 - state.left[1] * 0.25),
                            size * 0.34,
                            size * 0.34,
                        ],
                        color,
                    );
                }
            } else {
                let name = match control {
                    Control::Sprint => "ui/sprint",
                    Control::Attack => "ui/attack",
                    Control::Use => "ui/interact",
                    Control::Jump if self.swimming => "controls/waterascend",
                    Control::Sneak if self.swimming => "controls/waterdescend",
                    Control::Jump if self.vertical_movement => "controls/flyingascend",
                    Control::Sneak if self.vertical_movement => "controls/flyingdescend",
                    Control::Sneak if self.mounted => "controls/dismount",
                    Control::Jump if self.settings.mode == Mode::Dpad => "controls/jump_dpad",
                    Control::Sneak if self.settings.mode == Mode::Dpad => "controls/sneak_dpad",
                    Control::Jump => "ui/jump",
                    Control::Sneak => "ui/sneak",
                    Control::Pause => "ui/pause_icon",
                    Control::Chat => "gui",
                    Control::Inventory => "ui/more-dots",
                    Control::Settings => "ui/gear",
                    _ => continue,
                };
                let bounds = [x, y, size, size];
                match control {
                    Control::Chat => region(b, a, "gui", [200.0, 82.0, 18.0, 17.0], bounds, color),
                    Control::Inventory | Control::Settings => {
                        sprite(b, a, "ui/joystick_knob", bounds, color);
                        sprite(
                            b,
                            a,
                            name,
                            [x + size * 0.2, y + size * 0.2, size * 0.6, size * 0.6],
                            color,
                        );
                    }
                    Control::Pause => {
                        sprite(b, a, "ui/joystick_knob", bounds, color);
                        sprite(
                            b,
                            a,
                            name,
                            [x + size * 0.22, y + size * 0.22, size * 0.56, size * 0.56],
                            color,
                        );
                    }
                    _ => sprite(
                        b,
                        a,
                        &format!("{name}{}", if held { "_pressed" } else { "" }),
                        bounds,
                        color,
                    ),
                }
            }
            if let Some(e) = &self.editor
                && e.selected == control
            {
                let green = rgba(255, 255, 255, 255);
                for (p, q) in [
                    ([x - 1.0, y - 1.0], [x + size + 1.0, y - 1.0]),
                    ([x - 1.0, y + size + 1.0], [x + size + 1.0, y + size + 1.0]),
                    ([x - 1.0, y - 1.0], [x - 1.0, y + size + 1.0]),
                    ([x + size + 1.0, y - 1.0], [x + size + 1.0, y + size + 1.0]),
                ] {
                    line(b, a, p, q, green);
                }
            }
        }
        if self.editor.is_some() {
            self.draw_editor(b, a);
        }
    }
    /// Coordinates are fractions of the drawable surface. Returns a look delta.
    pub fn update(&mut self, id: u64, point: [f32; 2], started: bool, ended: bool) -> [f32; 2] {
        if ended {
            if let Some((
                Role::Look {
                    start,
                    since,
                    dragged,
                },
                _,
            )) = self.fingers.remove(&id)
                && !self.settings.crosshair()
                && !dragged
                && since.elapsed() < Duration::from_millis(350)
            {
                self.tap_aim = Some(start);
                self.pending.triggers[0] = 1.0;
            }
            return [0.0; 2];
        }
        if started {
            let control = Control::ALL.into_iter().rev().find(|c| {
                self.visible(*c)
                    && layout::contains(self.settings.placement(*c).rect(self.aspect()), point)
            });
            let role = match control {
                Some(Control::Move)
                    if !self
                        .fingers
                        .values()
                        .any(|(r, _)| matches!(r, Role::Move(_))) =>
                {
                    Role::Move(self.settings.placement(Control::Move).center)
                }
                Some(Control::Chat) => {
                    self.chat_requested = true;
                    Role::Ignore
                }
                Some(Control::Settings) => {
                    self.settings_requested = true;
                    Role::Ignore
                }
                Some(c) => Self::button(c).map_or(Role::Ignore, Role::Button),
                None => Role::Look {
                    start: point,
                    since: Instant::now(),
                    dragged: false,
                },
            };
            self.fingers.insert(id, (role, point));
            if let Role::Button(button) = role {
                match button {
                    Button::LeftTrigger => self.pending.triggers[0] = 1.0,
                    Button::RightTrigger => self.pending.triggers[1] = 1.0,
                    _ => self.pending.set(button, true),
                }
            }
            return [0.0; 2];
        }
        let aspect = self.aspect();
        if let Some((role, previous)) = self.fingers.get_mut(&id) {
            let delta = if let Role::Look { start, dragged, .. } = role {
                *dragged |= ((point[0] - start[0]) * aspect).hypot(point[1] - start[1]) > 0.018;
                if *dragged {
                    [point[0] - previous[0], point[1] - previous[1]]
                } else {
                    [0.0; 2]
                }
            } else {
                [0.0; 2]
            };
            *previous = point;
            return delta;
        }
        [0.0; 2]
    }
    pub fn clear(&mut self) {
        self.fingers.clear();
        self.pending = PadState::default();
        self.aim = None;
        self.tap_aim = None;
        self.chat_requested = false;
        self.settings_requested = false;
    }
    pub fn poll_state(&mut self) -> PadState {
        let mut state = self.state().merge(&std::mem::take(&mut self.pending));
        self.aim = self.tap_aim.take();
        if !self.settings.crosshair() {
            for (role, _) in self.fingers.values() {
                if let Role::Look {
                    start,
                    since,
                    dragged: false,
                } = role
                    && since.elapsed() >= Duration::from_millis(350)
                {
                    state.triggers[1] = 1.0;
                    self.aim = Some(*start);
                    break;
                }
            }
        }
        state
    }
    pub fn state(&self) -> PadState {
        let mut state = PadState::default();
        for (role, point) in self.fingers.values() {
            match role {
                Role::Move(anchor) => {
                    let radius = self.settings.placement(Control::Move).size / 2.0;
                    state.left = [
                        (point[0] - anchor[0]) * self.aspect() / radius,
                        (anchor[1] - point[1]) / radius,
                    ];
                    if self.settings.mode == Mode::Dpad {
                        state.left = state
                            .left
                            .map(|v| if v.abs() < 0.22 { 0.0 } else { v.signum() });
                    }
                    let length = state.left[0].hypot(state.left[1]).max(1.0);
                    state.left = state.left.map(|v| v / length);
                    if self.settings.mode != Mode::Dpad
                        && self.settings.sprint_with_joystick
                        && state.left[1] > 0.85
                    {
                        state.set(Button::LeftStick, true);
                    }
                }
                Role::Button(Button::LeftTrigger) => state.triggers[0] = 1.0,
                Role::Button(Button::RightTrigger) => state.triggers[1] = 1.0,
                Role::Button(b) => state.set(*b, true),
                _ => {}
            }
        }
        state
    }
}

fn nine(
    b: &mut HudBatch,
    a: &HudAtlas,
    name: &str,
    border: f32,
    [x, y, w, h]: [f32; 4],
    color: u32,
) {
    if let Some(r) = a.sprites.get(&format!("gui/bedrock/{name}")).copied() {
        let xs = [0.0, border, r.w as f32 - border, r.w as f32];
        let ys = [0.0, border, r.h as f32 - border, r.h as f32];
        let dx = [x, x + border, x + w - border, x + w];
        let dy = [y, y + border, y + h - border, y + h];
        for row in 0..3 {
            for col in 0..3 {
                let mut cell = r;
                cell.u0 = r.u0 + (r.u1 - r.u0) * xs[col] / r.w as f32;
                cell.u1 = r.u0 + (r.u1 - r.u0) * xs[col + 1] / r.w as f32;
                cell.v0 = r.v0 + (r.v1 - r.v0) * ys[row] / r.h as f32;
                cell.v1 = r.v0 + (r.v1 - r.v0) * ys[row + 1] / r.h as f32;
                b.rect_uv(
                    dx[col],
                    dy[row],
                    dx[col + 1] - dx[col],
                    dy[row + 1] - dy[row],
                    cell,
                    color,
                );
            }
        }
    }
}

fn sprite(b: &mut HudBatch, a: &HudAtlas, name: &str, [x, y, w, h]: [f32; 4], color: u32) {
    b.sprite_scaled(a, &format!("gui/bedrock/{name}"), x, y, w, h, color);
}
fn region(
    b: &mut HudBatch,
    a: &HudAtlas,
    name: &str,
    [sx, sy, sw, sh]: [f32; 4],
    [x, y, w, h]: [f32; 4],
    color: u32,
) {
    if let Some(mut r) = a.sprites.get(&format!("gui/bedrock/{name}")).copied() {
        let (u, v) = (r.u0, r.v0);
        let (du, dv) = ((r.u1 - u) / r.w as f32, (r.v1 - v) / r.h as f32);
        r.u0 = u + sx * du;
        r.u1 = u + (sx + sw) * du;
        r.v0 = v + sy * dv;
        r.v1 = v + (sy + sh) * dv;
        b.rect_uv(x, y, w, h, r, color);
    }
}

fn line(
    b: &mut crate::hud::HudBatch,
    a: &crate::hud::HudAtlas,
    p: [f32; 2],
    q: [f32; 2],
    color: u32,
) {
    let steps = (q[0] - p[0]).abs().max((q[1] - p[1]).abs()).ceil().max(1.0) as u32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        b.fill(
            a,
            p[0] + (q[0] - p[0]) * t - 0.7,
            p[1] + (q[1] - p[1]) * t - 0.7,
            1.4,
            1.4,
            color,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_tap_survives_one_poll_and_shared_buttons_release_independently() {
        let mut t = TouchControls::default();
        t.update(1, [0.92, 0.78], true, false);
        t.update(1, [0.92, 0.78], false, true);
        assert!(t.poll_state().held(Button::South));
        assert!(!t.poll_state().held(Button::South));
        t.update(2, [0.80, 0.64], true, false);
        t.update(3, [0.80, 0.64], true, false);
        t.update(2, [0.80, 0.64], false, true);
        assert_eq!(t.state().triggers[1], 1.0);
        t.update(3, [0.80, 0.64], false, true);
        assert_eq!(t.state().triggers[1], 0.0);
    }
    #[test]
    fn joystick_camera_and_jump_have_independent_owners() {
        let mut t = TouchControls::default();
        t.update(1, [0.13, 0.76], true, false);
        t.update(1, [0.20, 0.64], false, false);
        t.update(2, [0.6, 0.4], true, false);
        let d = t.update(2, [0.65, 0.45], false, false);
        assert!((d[0] - 0.05).abs() < 0.0001);
        t.update(3, [0.92, 0.78], true, false);
        assert!(t.state().held(Button::South));
        assert!(t.state().left[0] > 0.6);
        t.update(4, [0.09, 0.76], true, false);
        t.update(4, [0.02, 0.70], false, false);
        assert!(t.state().left[0] > 0.6);
        t.clear();
        assert_eq!(t.poll_state(), PadState::default());
    }
    #[test]
    fn tap_aim_and_camera_drag_are_distinct() {
        let mut t = TouchControls::default();
        t.settings.mode = Mode::Tap;
        t.update(1, [0.6, 0.4], true, false);
        t.update(1, [0.6, 0.4], false, true);
        assert_eq!(t.poll_state().triggers[0], 1.0);
        assert_eq!(t.aim, Some([0.6, 0.4]));
        assert_eq!(t.poll_state().triggers[0], 0.0);
        assert_eq!(t.aim, None);
        t.update(2, [0.6, 0.4], true, false);
        t.update(2, [0.7, 0.4], false, false);
        t.update(2, [0.7, 0.4], false, true);
        assert_eq!(t.poll_state().triggers, [0.0; 2]);
    }
    #[test]
    fn long_hold_mines_but_drag_cancels_it() {
        let mut t = TouchControls::default();
        t.settings.mode = Mode::Tap;
        t.update(1, [0.6, 0.4], true, false);
        if let Some((Role::Look { since, .. }, _)) = t.fingers.get_mut(&1) {
            *since = Instant::now() - Duration::from_millis(400);
        }
        assert_eq!(t.poll_state().triggers[1], 1.0);
        assert_eq!(t.aim, Some([0.6, 0.4]));
        t.update(1, [0.7, 0.4], false, false);
        assert_eq!(t.poll_state().triggers[1], 0.0);
        assert_eq!(t.aim, None);
    }
    #[test]
    fn dpad_can_slide_between_directions_and_mirror() {
        let mut t = TouchControls::default();
        t.settings.mode = Mode::Dpad;
        t.update(1, [0.13, 0.66], true, false);
        assert_eq!(t.state().left, [0.0, 1.0]);
        t.update(1, [0.20, 0.76], false, false);
        assert_eq!(t.state().left, [1.0, 0.0]);
        t.clear();
        t.update(10, [0.13, 0.76], true, false);
        assert!(t.state().held(Button::RightStick));
        t.clear();
        t.settings.swap_jump_sneak = true;
        t.update(11, [0.13, 0.76], true, false);
        assert!(t.state().held(Button::South));
        t.clear();
        t.settings.left_handed = true;
        t.update(2, [0.87, 0.66], true, false);
        assert_eq!(t.state().left, [0.0, 1.0]);
    }
    #[test]
    fn editing_owns_input_and_cancel_restores_layouts() {
        let mut t = TouchControls::default();
        t.open_editor();
        t.edit_touch(1, [0.13, 0.76], true, false);
        t.edit_touch(1, [0.18, 0.70], false, true);
        assert!((t.settings.placement(Control::Move).center[0] - 0.18).abs() < 0.001);
        assert_eq!(t.poll_state(), PadState::default());
        t.cancel_editor();
        assert_eq!(t.settings.placement(Control::Move).center, [0.13, 0.76]);
        t.open_editor();
        t.edit_touch(2, [0.5, 0.16], true, true);
        assert_eq!(t.settings.mode, Mode::Tap);
        t.edit_touch(3, [0.65, 0.70], true, true);
        assert!(t.settings.placement(Control::Move).size > 0.4);
        t.cancel_editor();
        assert_eq!(t.settings.mode, Mode::Crosshair);
    }
    #[test]
    fn layouts_roundtrip_and_reject_invalid_sizes() {
        let mut s = Settings::default();
        s.mode = Mode::Dpad;
        s.layouts[2][2].center = [0.4, 0.5];
        let bytes = serde_json::to_vec(&s).unwrap();
        let mut r: Settings = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(r.mode, Mode::Dpad);
        assert_eq!(r.layouts[2][2].center, [0.4, 0.5]);
        r.layouts[2][2].size = f32::NAN;
        r.sensitivity = f32::INFINITY;
        r.sanitize();
        assert!(r.layouts[2][2].size.is_finite());
        assert_eq!(r.sensitivity, 1.0);
        let rect = r.placement(Control::Jump).rect(20.0 / 9.0);
        assert!((rect[2] * (20.0 / 9.0) - rect[3]).abs() < 0.0001);
    }
    #[test]
    fn projection_targets_screen_point_without_turning_camera() {
        let forward = [0.0, 0.0, 1.0];
        let up = [0.0, 1.0, 0.0];
        assert_eq!(
            screen_direction(forward, up, [0.5, 0.5], 2.0, 90.0),
            forward
        );
        let ray = screen_direction(forward, up, [0.75, 0.25], 2.0, 90.0);
        assert!(ray[0] < 0.0 && ray[1] > 0.0 && ray[2] > 0.0);
        assert!((ray.iter().map(|x| x * x).sum::<f64>() - 1.0).abs() < 1e-10);
    }
    #[test]
    fn hotbar_all_nine_slots() {
        for i in 0..9 {
            assert_eq!(
                hotbar_slot(427.0, 240.0, [133.5 + i as f32 * 20.0, 230.0]),
                Some(i)
            );
        }
        assert_eq!(hotbar_slot(427.0, 240.0, [320.0, 230.0]), None);
    }
}
