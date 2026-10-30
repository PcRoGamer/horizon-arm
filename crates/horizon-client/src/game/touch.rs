//! Android touchscreen HUD.
//!
//! The default presentation follows Bedrock mobile's interaction model rather
//! than emulating a desktop mouse and keyboard.  Fingers are independent:
//! movement, camera, hotbar and action touches can all be held at once.
//!
//! `horizon-touch.txt` accepts Bedrock-style options (one per line):
//! `mode=joystick|crosshair|dpad`, `static_joystick=true|false`,
//! `show_action_buttons=true|false`, `left_handed=true|false`, and
//! `joystick_visibility=always|touch|hidden`.  The old `mobile` and `classic`
//! one-word files continue to map to joystick/tap and joystick/crosshair.

use super::{Game, Screen, touch_icons};
use horizon_client::hud::HudBatch;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, TouchPhase, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
    window::WindowId,
};

const STICK_DEADZONE: f32 = 0.12;
const SLOP: f32 = 1.24;
const HOTBAR_WIDTH: f32 = 182.0;
const HOTBAR_SLOT: f32 = 20.0;
const TAP_TRAVEL: f64 = 8.0;
const HOLD_TRAVEL: f64 = 11.0;
const ATTACK_HOLD: Duration = Duration::from_millis(175);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Preset {
    /// Bedrock "Joystick & tap to interact".
    JoystickTap,
    /// Bedrock "Joystick & aim crosshair".
    JoystickCrosshair,
    /// Bedrock "D-Pad & tap to interact".
    DpadTap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JoystickVisibility {
    Always,
    WhenTouched,
    Hidden,
}

#[derive(Clone, Copy, Debug)]
struct Settings {
    preset: Preset,
    static_joystick: bool,
    show_action_buttons: bool,
    left_handed: bool,
    joystick_visibility: JoystickVisibility,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            preset: Preset::JoystickTap,
            static_joystick: false,
            show_action_buttons: true,
            left_handed: false,
            joystick_visibility: JoystickVisibility::Always,
        }
    }
}

impl Settings {
    fn load() -> Self {
        let configured = std::env::var("HORIZON_TOUCH_PRESET")
            .ok()
            .or_else(|| std::fs::read_to_string("horizon-touch.txt").ok())
            .unwrap_or_default();
        let mut out = Self::default();
        let simple = configured.trim().to_ascii_lowercase();
        match simple.as_str() {
            "classic" | "crosshair" => out.preset = Preset::JoystickCrosshair,
            "dpad" | "d-pad" => out.preset = Preset::DpadTap,
            "mobile" | "joystick" => out.preset = Preset::JoystickTap,
            _ => {
                for raw in configured.lines() {
                    let Some((key, value)) = raw.split_once('=') else {
                        continue;
                    };
                    let key = key.trim().to_ascii_lowercase();
                    let value = value.trim().to_ascii_lowercase();
                    let yes = matches!(value.as_str(), "1" | "true" | "yes" | "on");
                    match key.as_str() {
                        "mode" | "control_mode" => {
                            out.preset = match value.as_str() {
                                "crosshair" | "joystick_crosshair" | "joystick & aim crosshair" => {
                                    Preset::JoystickCrosshair
                                }
                                "dpad" | "d-pad" | "dpad_tap" | "d-pad & tap to interact" => {
                                    Preset::DpadTap
                                }
                                _ => Preset::JoystickTap,
                            };
                        }
                        "static_joystick" => out.static_joystick = yes,
                        "show_action_buttons" => out.show_action_buttons = yes,
                        "left_handed" | "left_handed_inventory" => out.left_handed = yes,
                        "joystick_visibility" => {
                            out.joystick_visibility = match value.as_str() {
                                "hidden" | "never" => JoystickVisibility::Hidden,
                                "touch" | "when_touched" | "active" => JoystickVisibility::WhenTouched,
                                _ => JoystickVisibility::Always,
                            };
                        }
                        _ => {}
                    }
                }
            }
        }
        out
    }

    fn tap_interaction(self) -> bool {
        self.preset != Preset::JoystickCrosshair || !self.show_action_buttons
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Control {
    Jump,
    Sneak,
    Sprint,
    Attack,
    Use,
    Inventory,
    Chat,
    Pause,
    Perspective,
}

impl Control {
    fn label(self) -> &'static str {
        match self {
            Self::Jump => "jump",
            Self::Sneak => "sneak",
            Self::Sprint => "sprint",
            Self::Attack => "attack",
            Self::Use => "use",
            Self::Inventory => "inventory",
            Self::Chat => "chat",
            Self::Pause => "pause",
            Self::Perspective => "perspective",
        }
    }

    /// These fire once on touch-down.  Releasing them must not reopen/close the
    /// screen or cycle perspective a second time.
    fn momentary(self) -> bool {
        matches!(self, Self::Inventory | Self::Chat | Self::Pause | Self::Perspective)
    }
}

struct Layout {
    default_stick: (f32, f32),
    stick_radius: f32,
    dpad_center: (f32, f32),
    dpad_radius: f32,
    buttons: Vec<(Control, (f32, f32), f32)>,
    width: f32,
    height: f32,
    settings: Settings,
}

impl Layout {
    fn new((w, h): (f32, f32), settings: Settings) -> Self {
        use Control::*;

        // GUI pixels, not raw display pixels.  These bounds intentionally leave
        // a generous landscape safe zone for rounded corners / camera holes.
        let u = (h / 360.0).clamp(0.82, 1.30);
        let side = (w * 0.026).clamp(9.0, 24.0);
        let top = (h * 0.026).clamp(8.0, 18.0);
        let stick_radius = 50.0 * u;
        let dpad_radius = 49.0 * u;
        let action = 25.0 * u;
        let jump = 30.0 * u;
        let small = 17.0 * u;
        let bottom = h - 13.0 * u;
        let action_x = |from_edge: f32| {
            if settings.left_handed {
                side + from_edge * u
            } else {
                w - side - from_edge * u
            }
        };

        // Menu buttons stay on the top edge in every Bedrock touch mode.
        let mut buttons = vec![
            (Pause, (w * 0.5 - 47.0 * u, top + small), small),
            (Chat, (w * 0.5, top + small), small),
            (Perspective, (w * 0.5 + 47.0 * u, top + small), small),
        ];

        // Inventory access is beside the hotbar rather than mixed into the
        // top controls.  Left-handed mode mirrors it along with gameplay input.
        let inventory_x = if settings.left_handed {
            w * 0.5 - HOTBAR_WIDTH * 0.5 - 25.0 * u
        } else {
            w * 0.5 + HOTBAR_WIDTH * 0.5 + 25.0 * u
        };
        buttons.push((Inventory, (inventory_x, h - 20.0 * u), small));

        // Common Bedrock action cluster.
        buttons.push((Jump, (action_x(34.0), bottom - 35.0 * u), jump));
        buttons.push((Sneak, (action_x(103.0), bottom - 99.0 * u), action));
        buttons.push((Sprint, (action_x(119.0), bottom - 31.0 * u), action * 0.92));

        if settings.preset == Preset::JoystickCrosshair && settings.show_action_buttons {
            buttons.push((Use, (action_x(36.0), bottom - 117.0 * u), action));
            buttons.push((Attack, (action_x(183.0), bottom - 42.0 * u), action));
        }

        let movement_x = if settings.left_handed {
            w - side - stick_radius
        } else {
            side + stick_radius
        };
        let movement_y = h - 18.0 * u - stick_radius;

        Self {
            default_stick: (movement_x, movement_y),
            stick_radius,
            dpad_center: (movement_x, movement_y),
            dpad_radius,
            buttons,
            width: w,
            height: h,
            settings,
        }
    }

    fn button_at(&self, pos: (f64, f64)) -> Option<Control> {
        let hit = |(cx, cy): (f32, f32), r: f32| {
            let (dx, dy) = (pos.0 - f64::from(cx), pos.1 - f64::from(cy));
            dx * dx + dy * dy <= f64::from(r * SLOP).powi(2)
        };
        self.buttons
            .iter()
            .rev()
            .find(|(_, centre, r)| hit(*centre, *r))
            .map(|(c, _, _)| *c)
    }

    /// Floating joystick activation region.  Left-handed mode mirrors it.
    fn on_stick(&self, pos: (f64, f64)) -> bool {
        if self.settings.preset == Preset::DpadTap {
            return false;
        }
        let on_side = if self.settings.left_handed {
            pos.0 > f64::from(self.width * 0.54)
        } else {
            pos.0 < f64::from(self.width * 0.46)
        };
        on_side && pos.1 > f64::from(self.height * 0.40)
    }

    /// Digital D-pad vector in the same coordinate convention as the analog
    /// stick: +x right, +y forward/up-screen.
    fn dpad_vector(&self, pos: (f64, f64)) -> Option<[f32; 2]> {
        if self.settings.preset != Preset::DpadTap {
            return None;
        }
        let dx = pos.0 as f32 - self.dpad_center.0;
        let dy = pos.1 as f32 - self.dpad_center.1;
        let reach = self.dpad_radius * 1.18;
        if dx.abs() > reach || dy.abs() > reach {
            return None;
        }
        let dead = self.dpad_radius * 0.18;
        let x = if dx > dead { 1.0 } else if dx < -dead { -1.0 } else { 0.0 };
        let y = if dy > dead { -1.0 } else if dy < -dead { 1.0 } else { 0.0 };
        if x == 0.0 && y == 0.0 {
            return Some([0.0, 0.0]);
        }
        if x != 0.0 && y != 0.0 {
            let d = std::f32::consts::FRAC_1_SQRT_2;
            Some([x * d, y * d])
        } else {
            Some([x, y])
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Gesture {
    Look {
        last: (f64, f64),
        travel: f64,
        started: Instant,
        attacking: bool,
    },
    Stick { origin: (f64, f64) },
    Dpad,
    Button(Control),
    Hotbar,
}

/// Every finger has an independent role.  Moving never stops because another
/// thumb looks, jumps, changes hotbar slot or presses an action button.
pub(super) struct Touch {
    fingers: HashMap<u64, Gesture>,
    stick: [f32; 2],
    stick_origin: Option<(f32, f32)>,
    pressed: Vec<Control>,
    settings: Settings,
}

impl Default for Touch {
    fn default() -> Self {
        Self {
            fingers: HashMap::new(),
            stick: [0.0; 2],
            stick_origin: None,
            pressed: Vec::new(),
            settings: Settings::load(),
        }
    }
}

impl Game<'_> {
    pub(super) fn touch(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: WindowId,
        touch: &winit::event::Touch,
    ) {
        // F1 may hide the HUD, but it must not turn the touchscreen into a
        // menu pointer and strand the player without movement.
        let world = self.screen == Screen::Playing && self.title.is_none() && self.player.is_some();
        if world {
            self.touch_world(event_loop, window, touch);
        } else {
            if self.screen != Screen::Chat {
                self.set_soft_keyboard(false);
            }
            self.touch_pointer(event_loop, window, touch);
        }
    }

    pub(super) fn touch_stick(&self) -> Option<[f32; 2]> {
        (self.touch_ui.stick != [0.0, 0.0]).then_some(self.touch_ui.stick)
    }

    fn touch_world(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: WindowId,
        touch: &winit::event::Touch,
    ) {
        let batch = HudBatch::new(self.touch_pixels());
        let settings = self.touch_ui.settings;
        let layout = Layout::new((batch.width, batch.height), settings);
        let scale = f64::from(batch.scale);
        let pos = (touch.location.x / scale, touch.location.y / scale);
        let id = touch.id;

        match touch.phase {
            TouchPhase::Started => {
                if !self.captured {
                    self.capture(true);
                }

                if let Some(control) = layout.button_at(pos) {
                    if control.momentary() {
                        self.press(control, true);
                        if self.screen != Screen::Playing {
                            self.release_touch_inputs();
                            return;
                        }
                    }
                    if !self.touch_ui.pressed.contains(&control) {
                        self.touch_ui.pressed.push(control);
                        self.press(control, true);
                    }
                    self.touch_ui.fingers.insert(id, Gesture::Button(control));
                    return;
                }

                let gesture = if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                    self.selected = slot;
                    Gesture::Hotbar
                } else if let Some(vector) = layout.dpad_vector(pos) {
                    self.touch_ui.stick = vector;
                    Gesture::Dpad
                } else if layout.on_stick(pos)
                    && !self
                        .touch_ui
                        .fingers
                        .values()
                        .any(|g| matches!(g, Gesture::Stick { .. }))
                {
                    let origin = if settings.static_joystick {
                        (f64::from(layout.default_stick.0), f64::from(layout.default_stick.1))
                    } else {
                        pos
                    };
                    self.touch_ui.stick_origin = Some((origin.0 as f32, origin.1 as f32));
                    Gesture::Stick { origin }
                } else {
                    look_at(pos)
                };
                self.touch_ui.fingers.insert(id, gesture);
            }
            TouchPhase::Moved => {
                // Remove temporarily so mouse/key helpers can mutably borrow Game.
                let Some(mut gesture) = self.touch_ui.fingers.remove(&id) else {
                    return;
                };
                match &mut gesture {
                    Gesture::Look { last, travel, .. } => {
                        let (dx, dy) = (pos.0 - last.0, pos.1 - last.1);
                        *travel += (dx * dx + dy * dy).sqrt();
                        *last = pos;
                        self.mouse.on_move(0.0, 0.0, dx, dy, true, true);
                        self.last_input = Instant::now();
                    }
                    Gesture::Stick { origin } => {
                        let push = [
                            (pos.0 - origin.0) as f32 / layout.stick_radius,
                            -((pos.1 - origin.1) as f32 / layout.stick_radius),
                        ];
                        self.touch_ui.stick =
                            horizon_client::gamepad::move_vector(push, STICK_DEADZONE)
                                .unwrap_or([0.0; 2]);
                    }
                    Gesture::Dpad => {
                        self.touch_ui.stick = layout.dpad_vector(pos).unwrap_or([0.0, 0.0]);
                    }
                    Gesture::Hotbar => {
                        if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                            self.selected = slot;
                        }
                    }
                    Gesture::Button(_) => {}
                }
                self.touch_ui.fingers.insert(id, gesture);
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let Some(gesture) = self.touch_ui.fingers.remove(&id) else {
                    return;
                };
                match gesture {
                    Gesture::Look { travel, attacking, .. } => {
                        if attacking {
                            let _ = self.play_mouse(MouseButton::Left, false);
                        } else if touch.phase == TouchPhase::Ended
                            && travel < TAP_TRAVEL
                            && settings.tap_interaction()
                        {
                            self.touch_click(event_loop, window, touch);
                        }
                    }
                    Gesture::Hotbar => {}
                    Gesture::Stick { .. } | Gesture::Dpad => {
                        self.touch_ui.stick = [0.0; 2];
                        self.touch_ui.stick_origin = None;
                    }
                    Gesture::Button(control) => {
                        let still_down = self
                            .touch_ui
                            .fingers
                            .values()
                            .any(|g| matches!(g, Gesture::Button(c) if *c == control));
                        if !still_down {
                            self.touch_ui.pressed.retain(|c| *c != control);
                            self.press(control, false);
                        }
                    }
                }
            }
        }
    }

    /// Tap modes: a stationary world touch becomes held left-click after a
    /// short delay.  A quick tap attacks once; moving remains pure camera look.
    fn update_touch_holds(&mut self) {
        if !self.touch_ui.settings.tap_interaction() {
            return;
        }
        let now = Instant::now();
        let candidate = self.touch_ui.fingers.iter().find_map(|(id, gesture)| match gesture {
            Gesture::Look {
                travel,
                started,
                attacking: false,
                ..
            } if *travel < HOLD_TRAVEL && now.duration_since(*started) >= ATTACK_HOLD => Some(*id),
            _ => None,
        });
        let Some(id) = candidate else { return };
        if self.play_mouse(MouseButton::Left, true).is_ok()
            && let Some(Gesture::Look { attacking, .. }) = self.touch_ui.fingers.get_mut(&id)
        {
            *attacking = true;
        }
    }

    pub(super) fn release_touch_inputs(&mut self) {
        self.touch_ui.stick = [0.0; 2];
        self.touch_ui.stick_origin = None;
        self.touch_ui.fingers.clear();
        let controls = std::mem::take(&mut self.touch_ui.pressed);
        for control in controls {
            if !control.momentary() {
                self.press(control, false);
            }
        }
        let _ = self.play_mouse(MouseButton::Left, false);
    }

    fn touch_click(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: WindowId,
        touch: &winit::event::Touch,
    ) {
        let device_id = touch.device_id;
        let press = |state| WindowEvent::MouseInput {
            device_id,
            state,
            button: MouseButton::Left,
        };
        let position: PhysicalPosition<f64> = touch.location;
        self.window_event(event_loop, window, WindowEvent::CursorMoved { device_id, position });
        self.window_event(event_loop, window, press(ElementState::Pressed));
        self.window_event(event_loop, window, press(ElementState::Released));
    }

    fn touch_pointer(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: WindowId,
        touch: &winit::event::Touch,
    ) {
        let (x, y) = (touch.location.x, touch.location.y);
        let device_id = touch.device_id;
        let position = touch.location;
        let press = |state| WindowEvent::MouseInput {
            device_id,
            state,
            button: MouseButton::Left,
        };
        let last = match (touch.phase, self.touch) {
            (TouchPhase::Started, _) => {
                self.touch = Some((touch.id, (x, y)));
                self.window_event(event_loop, window, WindowEvent::CursorMoved { device_id, position });
                self.window_event(event_loop, window, press(ElementState::Pressed));
                return;
            }
            (TouchPhase::Cancelled, _) => {
                self.touch = None;
                self.window_event(event_loop, window, press(ElementState::Released));
                return;
            }
            (TouchPhase::Moved | TouchPhase::Ended, Some((finger, last))) if finger == touch.id => {
                Some(last)
            }
            _ => None,
        };
        if let Some((last_x, last_y)) = last {
            self.window_event(event_loop, window, WindowEvent::CursorMoved { device_id, position });
            if touch.phase == TouchPhase::Ended {
                self.touch = None;
                self.window_event(event_loop, window, press(ElementState::Released));
            } else {
                self.touch = Some((touch.id, (x, y)));
                self.device_event(
                    event_loop,
                    device_id,
                    winit::event::DeviceEvent::MouseMotion {
                        delta: (x - last_x, y - last_y),
                    },
                );
            }
        }
    }

    fn press(&mut self, control: Control, down: bool) {
        let result = match control {
            Control::Jump => self.mapped_key(KeyCode::Space, down, false),
            Control::Sneak => self.mapped_key(KeyCode::ShiftLeft, down, false),
            Control::Sprint => self.mapped_key(KeyCode::ControlLeft, down, false),
            Control::Attack => self.play_mouse(MouseButton::Left, down),
            Control::Use => self.play_mouse(MouseButton::Right, down),
            Control::Inventory if down => self.key_pressed(KeyCode::KeyE),
            Control::Chat if down => {
                let result = self.key_pressed(KeyCode::KeyT);
                if result.is_ok() && self.screen == Screen::Chat {
                    self.set_soft_keyboard(true);
                }
                result
            }
            Control::Pause if down => self.key_pressed(KeyCode::Escape),
            Control::Perspective if down => self.key_pressed(KeyCode::F5),
            Control::Inventory | Control::Chat | Control::Pause | Control::Perspective => Ok(()),
        };
        if let Err(e) = result {
            log::warn!("Horizon: on-screen {}: {e}", control.label());
        }
    }

    pub(super) fn set_soft_keyboard(&self, show: bool) {
        if let Some(window) = &self.window {
            window.set_ime_allowed(show);
        }
    }

    fn touch_pixels(&self) -> [u32; 2] {
        self.renderer.as_ref().map_or_else(
            || {
                self.window
                    .as_ref()
                    .map(|w| w.inner_size().into())
                    .unwrap_or([1, 1])
            },
            |r| [r.config.width, r.config.height],
        )
    }

    /// Bedrock-style mobile HUD.  The game remains visible and every held
    /// control has its own visual state.  Menu buttons stay at the top edge,
    /// inventory sits beside the hotbar, and movement can be joystick or D-pad.
    pub(super) fn draw_touch_controls(&mut self, batch: &mut HudBatch) {
        self.set_soft_keyboard(false);
        self.update_touch_holds();

        if self.ui.hide_gui {
            return;
        }
        let settings = self.touch_ui.settings;
        let layout = Layout::new((batch.width, batch.height), settings);
        let atlas = &self.gui.atlas;

        match settings.preset {
            Preset::DpadTap => touch_icons::dpad(
                batch,
                atlas,
                layout.dpad_center.0,
                layout.dpad_center.1,
                layout.dpad_radius,
                self.touch_ui.stick,
            ),
            Preset::JoystickTap | Preset::JoystickCrosshair => {
                let active = self.touch_ui.stick_origin.is_some();
                let visible = match settings.joystick_visibility {
                    JoystickVisibility::Always => true,
                    JoystickVisibility::WhenTouched => active,
                    JoystickVisibility::Hidden => false,
                };
                if visible {
                    let (sx, sy) = if settings.static_joystick {
                        layout.default_stick
                    } else {
                        self.touch_ui.stick_origin.unwrap_or(layout.default_stick)
                    };
                    touch_icons::joystick(batch, atlas, sx, sy, layout.stick_radius, self.touch_ui.stick);
                }
            }
        }

        for (control, (cx, cy), r) in &layout.buttons {
            let down = self.touch_ui.pressed.contains(control);
            let icon = match control {
                Control::Jump => touch_icons::Icon::Jump,
                Control::Sneak => touch_icons::Icon::Sneak,
                Control::Sprint => touch_icons::Icon::Sprint,
                Control::Attack => touch_icons::Icon::Attack,
                Control::Use => touch_icons::Icon::Use,
                Control::Inventory => touch_icons::Icon::Inventory,
                Control::Chat => touch_icons::Icon::Chat,
                Control::Pause => touch_icons::Icon::Pause,
                Control::Perspective => touch_icons::Icon::Perspective,
            };
            touch_icons::button(batch, atlas, icon, *cx, *cy, *r, down);
        }
    }
}

fn look_at(pos: (f64, f64)) -> Gesture {
    Gesture::Look {
        last: pos,
        travel: 0.0,
        started: Instant::now(),
        attacking: false,
    }
}

fn hotbar_slot(pos: (f64, f64), (w, h): (f32, f32)) -> Option<usize> {
    let x0 = f64::from(w / 2.0 - HOTBAR_WIDTH / 2.0);
    let y0 = f64::from(h - 25.0);
    if pos.0 >= x0
        && pos.0 < x0 + f64::from(HOTBAR_WIDTH)
        && pos.1 >= y0
        && pos.1 <= f64::from(h)
    {
        Some((((pos.0 - x0) / f64::from(HOTBAR_SLOT)) as usize).min(8))
    } else {
        None
    }
}
