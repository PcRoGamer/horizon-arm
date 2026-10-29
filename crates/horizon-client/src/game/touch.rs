//! Android touchscreen HUD.
//!
//! Mobile is deliberately not a virtual keyboard/mouse overlay.  Fingers have
//! independent jobs: a floating left movement stick, direct camera look on the
//! rest of the world, mobile-sized action controls and direct hotbar selection.
//! The default preset keeps the screen quiet (tap/hold the world to attack);
//! `horizon-touch.txt` containing `classic` restores a separate HIT button.
use super::{Game, Screen};
use horizon_client::hud::{HudBatch, rgba};
use std::{collections::HashMap, time::{Duration, Instant}};
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
    Mobile,
    Classic,
}

impl Preset {
    fn load() -> Self {
        let configured = std::env::var("HORIZON_TOUCH_PRESET")
            .ok()
            .or_else(|| std::fs::read_to_string("horizon-touch.txt").ok())
            .unwrap_or_default();
        if configured.trim().eq_ignore_ascii_case("classic") {
            Self::Classic
        } else {
            Self::Mobile
        }
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
}

impl Control {
    fn label(self) -> &'static str {
        match self {
            Control::Jump => "JUMP",
            Control::Sneak => "SNEAK",
            Control::Sprint => "SPRINT",
            Control::Attack => "HIT",
            Control::Use => "USE",
            Control::Inventory => "ITEMS",
            Control::Chat => "CHAT",
            Control::Pause => "MENU",
        }
    }

    /// These are actions, not held keys.  Firing them on touch release as well
    /// was enough to immediately close screens on some devices.
    fn momentary(self) -> bool {
        matches!(self, Self::Inventory | Self::Chat | Self::Pause)
    }
}

struct Layout {
    default_stick: (f32, f32),
    stick_radius: f32,
    buttons: Vec<(Control, (f32, f32), f32)>,
    width: f32,
    height: f32,
}

impl Layout {
    fn new((w, h): (f32, f32), preset: Preset) -> Self {
        use Control::*;

        // HudBatch dimensions are GUI pixels rather than raw phone pixels.  A
        // modest scale keeps controls reachable on a small phone without
        // becoming enormous on tablets.  The side margin also keeps the common
        // landscape camera-hole / rounded-corner zones clear without requiring
        // Android-specific inset plumbing in the renderer.
        let u = (h / 360.0).clamp(0.82, 1.28);
        let side = (w * 0.025).clamp(8.0, 22.0);
        let top = (h * 0.025).clamp(7.0, 16.0);
        let r = 54.0 * u;
        let action = 27.0 * u;
        let jump = 32.0 * u;
        let small = 21.0 * u;
        let right = w - side;
        let bottom = h - 14.0 * u;

        let mut buttons = vec![
            (Jump, (right - jump, bottom - jump), jump),
            (Sprint, (right - 140.0 * u, bottom - 30.0 * u), action),
            (Use, (right - 28.0 * u, bottom - 118.0 * u), action),
            (Sneak, (right - 116.0 * u, bottom - 114.0 * u), action),
            (Inventory, (right - small, top + small), small),
            (Chat, (right - (small * 2.0 + 12.0 * u), top + small), small),
            (Pause, (right - (small * 4.0 + 24.0 * u), top + small), small),
        ];
        if preset == Preset::Classic {
            buttons.push((Attack, (right - 208.0 * u, bottom - 32.0 * u), action));
        }

        Self {
            default_stick: (side + r, h - 16.0 * u - r),
            stick_radius: r,
            buttons,
            width: w,
            height: h,
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

    /// Floating joystick activation region.  Starting anywhere in the lower
    /// left portion places the stick under the player's thumb.
    fn on_stick(&self, pos: (f64, f64)) -> bool {
        pos.0 < f64::from(self.width * 0.46) && pos.1 > f64::from(self.height * 0.40)
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
    Button(Control),
    Hotbar,
}

/// Every finger has an independent role.  This is essential on a phone: moving
/// must not stop when another thumb looks, jumps, uses an item or changes slot.
pub(super) struct Touch {
    fingers: HashMap<u64, Gesture>,
    stick: [f32; 2],
    stick_origin: Option<(f32, f32)>,
    pressed: Vec<Control>,
    preset: Preset,
}

impl Default for Touch {
    fn default() -> Self {
        Self {
            fingers: HashMap::new(),
            stick: [0.0; 2],
            stick_origin: None,
            pressed: Vec::new(),
            preset: Preset::load(),
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
        // F1 may hide the HUD, but it must not turn a phone's touchscreen into
        // a menu pointer and strand the player without movement.
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
        let preset = self.touch_ui.preset;
        let layout = Layout::new((batch.width, batch.height), preset);
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
                        // Opening chat/inventory/pause changes screens while the
                        // movement thumb may still be down.  Release its logical
                        // keys now; its later Android UP event belongs to the new
                        // screen and would otherwise leave movement stuck.
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
                } else if layout.on_stick(pos)
                    && !self
                        .touch_ui
                        .fingers
                        .values()
                        .any(|g| matches!(g, Gesture::Stick { .. }))
                {
                    self.touch_ui.stick_origin = Some((pos.0 as f32, pos.1 as f32));
                    Gesture::Stick { origin: pos }
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
                    Gesture::Look {
                        travel,
                        attacking,
                        ..
                    } => {
                        if attacking {
                            let _ = self.play_mouse(MouseButton::Left, false);
                        } else if touch.phase == TouchPhase::Ended && travel < TAP_TRAVEL {
                            self.touch_click(event_loop, window, touch);
                        }
                    }
                    Gesture::Hotbar => {}
                    Gesture::Stick { .. } => {
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

    /// Mobile preset: a stationary world touch becomes held left-click after a
    /// short delay.  That makes mining natural while a quick tap still attacks
    /// once and a moving touch remains pure camera look.
    fn update_touch_holds(&mut self) {
        if self.touch_ui.preset != Preset::Mobile {
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

    fn release_touch_inputs(&mut self) {
        self.touch_ui.stick = [0.0; 2];
        self.touch_ui.stick_origin = None;
        self.touch_ui.fingers.clear();
        let controls = std::mem::take(&mut self.touch_ui.pressed);
        for control in controls {
            if !control.momentary() {
                self.press(control, false);
            }
        }
        // A mobile look-hold is not represented in `pressed`.
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
        self.window_event(
            event_loop,
            window,
            WindowEvent::CursorMoved { device_id, position },
        );
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
                self.window_event(
                    event_loop,
                    window,
                    WindowEvent::CursorMoved { device_id, position },
                );
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
            self.window_event(
                event_loop,
                window,
                WindowEvent::CursorMoved { device_id, position },
            );
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
            Control::Inventory | Control::Chat | Control::Pause => Ok(()),
        };
        if let Err(e) = result {
            log::warn!("Horizon: on-screen {}: {e}", control.label());
        }
    }

    fn set_soft_keyboard(&self, show: bool) {
        if let Some(window) = &self.window {
            // winit 0.30.6+ maps this to Android's soft keyboard.  Android
            // key events populate KeyEvent::text, which Horizon's existing
            // chat editor already consumes (including Unicode/Arabic text).
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

    /// Render a low-obstruction mobile HUD.  The stick base appears under the
    /// movement thumb and every held action brightens independently.
    pub(super) fn draw_touch_controls(&mut self, batch: &mut HudBatch) {
        self.set_soft_keyboard(false);
        self.update_touch_holds();

        if self.ui.hide_gui {
            return;
        }
        let preset = self.touch_ui.preset;
        let layout = Layout::new((batch.width, batch.height), preset);
        let atlas = &self.gui.atlas;
        let (sx, sy) = self.touch_ui.stick_origin.unwrap_or(layout.default_stick);
        let r = layout.stick_radius;

        batch.fill(atlas, sx - r, sy - r, r * 2.0, r * 2.0, rgba(8, 8, 12, 52));
        let knob = r * 0.42;
        let moving = self.touch_ui.stick != [0.0, 0.0];
        batch.fill(
            atlas,
            sx + self.touch_ui.stick[0] * r - knob,
            sy - self.touch_ui.stick[1] * r - knob,
            knob * 2.0,
            knob * 2.0,
            rgba(255, 255, 255, if moving { 150 } else { 72 }),
        );

        for (control, (cx, cy), r) in &layout.buttons {
            let down = self.touch_ui.pressed.contains(control);
            batch.fill(
                atlas,
                cx - r,
                cy - r,
                r * 2.0,
                r * 2.0,
                rgba(8, 8, 12, if down { 130 } else { 58 }),
            );
            let inner = r - 2.0;
            batch.fill(
                atlas,
                cx - inner,
                cy - inner,
                inner * 2.0,
                inner * 2.0,
                if down {
                    rgba(255, 255, 255, 90)
                } else {
                    rgba(50, 50, 58, 54)
                },
            );
            let label = control.label();
            let width = HudBatch::text_width(atlas, label);
            batch.text(
                atlas,
                label,
                (cx - width / 2.0).floor(),
                (cy - 4.0).floor(),
                rgba(255, 255, 255, if down { 255 } else { 210 }),
                true,
            );
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
    let y0 = f64::from(h - 22.0);
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
