//! The touchscreen (Android): in the world a finger drives on-screen controls
//! (a stick, a look area and the buttons a keyboard would have had); in menus
//! it is a plain pointer, the mapping the port started with.
//!
//! The controls are laid out like the HUD, in GUI pixels, so they keep their
//! place whatever the screen's density. They feed the same key presses and
//! mouse buttons the physical keyboard does, so rebinds and screens need no
//! Android code of their own.
use super::{Game, Screen};
use horizon_client::hud::{HudBatch, rgba};
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, TouchPhase, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
    window::WindowId,
};

/// The stick's radius and deadzone, in GUI pixels.
const STICK_RADIUS: f32 = 56.0;
const STICK_DEADZONE: f32 = 0.12;
/// A press reaches this far past a button's edge (a thumb is not a cursor).
const SLOP: f32 = 1.15;
/// The hotbar: 182x22 GUI pixels at the bottom centre (`Hud.hotbar`).
const HOTBAR_WIDTH: f32 = 182.0;
const HOTBAR_SLOT: f32 = 20.0;

/// A button on screen. Each is the vanilla key or mouse button it stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Control {
    Jump,
    Sneak,
    Attack,
    Use,
    Inventory,
    Chat,
    Pause,
}

impl Control {
    fn label(self) -> &'static str {
        match self {
            Control::Jump => "Jump",
            Control::Sneak => "Sneak",
            Control::Attack => "Hit",
            Control::Use => "Use",
            Control::Inventory => "Items",
            Control::Chat => "Chat",
            Control::Pause => "Menu",
        }
    }
}

/// Where the controls sit for a window of `(width, height)` GUI pixels.
struct Layout {
    stick: (f32, f32),
    buttons: Vec<(Control, (f32, f32), f32)>,
}

impl Layout {
    fn new((w, h): (f32, f32)) -> Self {
        use Control::*;
        Self {
            stick: (STICK_RADIUS + 12.0, h - STICK_RADIUS - 12.0),
            buttons: vec![
                (Jump, (w - 66.0, h - 66.0), 32.0),
                (Attack, (w - 150.0, h - 54.0), 26.0),
                (Use, (w - 54.0, h - 150.0), 26.0),
                (Sneak, (w - 150.0, h - 136.0), 26.0),
                (Inventory, (w - 34.0, 32.0), 24.0),
                (Chat, (w - 92.0, 32.0), 24.0),
                (Pause, (w - 150.0, 32.0), 24.0),
            ],
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
            .map(|(control, _, _)| *control)
    }

    /// The stick answers a generous radius too: it is where the thumb rests.
    fn on_stick(&self, pos: (f64, f64)) -> bool {
        let (dx, dy) = (pos.0 - f64::from(self.stick.0), pos.1 - f64::from(self.stick.1));
        dx * dx + dy * dy <= f64::from(STICK_RADIUS * 1.6).powi(2)
    }
}

/// What the followed finger is doing.
#[derive(Clone, Copy, Debug)]
enum Gesture {
    /// Turning the view: the last GUI position and how far it has travelled.
    Look { last: (f64, f64), travel: f64 },
    /// Moving: where it went down.
    Stick { origin: (f64, f64) },
    /// Holding a button.
    Button(Control),
}

/// The touchscreen's state: one finger is followed (a second, a pinch, is
/// ignored), and the controls it drives.
pub(super) struct Touch {
    finger: Option<(u64, Gesture)>,
    /// The stick's direction, `[left, forward]`, as `Keys.analog` reads it.
    stick: [f32; 2],
    /// The buttons down, for their highlight.
    pressed: Vec<Control>,
}

impl Default for Touch {
    fn default() -> Self {
        Self { finger: None, stick: [0.0; 2], pressed: Vec::new() }
    }
}

impl Game<'_> {
    /// A touch: the controls in the world, the pointer in every screen.
    pub(super) fn touch(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: WindowId,
        touch: &winit::event::Touch,
    ) {
        let world = self.screen == Screen::Playing
            && self.title.is_none()
            && self.player.is_some()
            && !self.ui.hide_gui;
        if world {
            self.touch_world(event_loop, window, touch);
        } else {
            self.touch_pointer(event_loop, window, touch);
        }
    }

    /// The stick's push while a finger holds it (`Keys::analog`).
    pub(super) fn touch_stick(&self) -> Option<[f32; 2]> {
        (self.touch_ui.stick != [0.0, 0.0]).then_some(self.touch_ui.stick)
    }

    fn touch_world(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: WindowId,
        touch: &winit::event::Touch,
    ) {
        let size = self.touch_pixels();
        let batch = HudBatch::new(size);
        let layout = Layout::new((batch.width, batch.height));
        let scale = f64::from(batch.scale);
        let pos = (f64::from(touch.location.x) / scale, f64::from(touch.location.y) / scale);
        let id = touch.id;
        match touch.phase {
            TouchPhase::Started => {
                // Like a click into the window: the first touch takes the
                // pointer, and the one after it drives the controls.
                if !self.captured {
                    self.capture(true);
                    self.touch_ui.finger = Some((id, look_at(pos)));
                    return;
                }
                if let Some(control) = layout.button_at(pos) {
                    self.touch_ui.finger = Some((id, Gesture::Button(control)));
                    self.touch_ui.pressed.push(control);
                    self.press(control, true);
                } else if layout.on_stick(pos) {
                    self.touch_ui.finger = Some((id, Gesture::Stick { origin: pos }));
                } else if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                    // A tap on a hotbar slot picks it, as the number keys do.
                    self.selected = slot;
                } else {
                    self.touch_ui.finger = Some((id, look_at(pos)));
                }
            }
            TouchPhase::Moved => {
                let Some((finger, mut gesture)) = self.touch_ui.finger else {
                    return;
                };
                if finger != id {
                    return;
                }
                match &mut gesture {
                    Gesture::Look { last, travel } => {
                        let (dx, dy) = (pos.0 - last.0, pos.1 - last.1);
                        *travel += (dx * dx + dy * dy).sqrt();
                        *last = pos;
                        // MouseHandler.onMove: the view turns once the frame
                        // applies the accumulated movement.
                        self.mouse.on_move(0.0, 0.0, dx, dy, true, true);
                        self.last_input = Instant::now();
                    }
                    Gesture::Stick { origin } => {
                        let push = [
                            (pos.0 - origin.0) as f32 / STICK_RADIUS,
                            -((pos.1 - origin.1) as f32 / STICK_RADIUS),
                        ];
                        self.touch_ui.stick = horizon_client::gamepad::move_vector(push, STICK_DEADZONE)
                            .unwrap_or([0.0; 2]);
                    }
                    Gesture::Button(_) => {}
                }
                self.touch_ui.finger = Some((finger, gesture));
            }
            TouchPhase::Ended => {
                let Some((finger, gesture)) = self.touch_ui.finger.take() else {
                    return;
                };
                if finger != id {
                    self.touch_ui.finger = Some((finger, gesture));
                    return;
                }
                match gesture {
                    // A tap with no travel hits what the crosshair is on, as a
                    // mouse click does.
                    Gesture::Look { travel, .. } if travel < 8.0 => {
                        self.touch_click(event_loop, window, touch);
                    }
                    Gesture::Look { .. } => {}
                    Gesture::Stick { .. } => self.touch_ui.stick = [0.0; 2],
                    Gesture::Button(control) => {
                        self.touch_ui.pressed.retain(|c| *c != control);
                        self.press(control, false);
                    }
                }
            }
            TouchPhase::Cancelled => {
                let Some((finger, gesture)) = self.touch_ui.finger.take() else {
                    return;
                };
                if finger != id {
                    self.touch_ui.finger = Some((finger, gesture));
                    return;
                }
                if let Gesture::Button(control) = gesture {
                    self.touch_ui.pressed.retain(|c| *c != control);
                    self.press(control, false);
                }
                self.touch_ui.stick = [0.0; 2];
            }
        }
    }

    fn touch_click(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: WindowId,
        touch: &winit::event::Touch,
    ) {
        let device_id = touch.device_id;
        let press = |state| WindowEvent::MouseInput { device_id, state, button: MouseButton::Left };
        let position: PhysicalPosition<f64> = touch.location;
        self.window_event(
            event_loop,
            window,
            WindowEvent::CursorMoved { device_id, position },
        );
        self.window_event(event_loop, window, press(ElementState::Pressed));
        self.window_event(event_loop, window, press(ElementState::Released));
    }

    /// The touchscreen as a mouse, for menus and every other screen: a tap
    /// moves the cursor and presses the left button; a drag moves the cursor
    /// and, with the pointer grabbed, turns the view through relative motion,
    /// as a mouse's does. Only the finger that began the touch is followed.
    fn touch_pointer(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: WindowId,
        touch: &winit::event::Touch,
    ) {
        let (x, y) = (touch.location.x, touch.location.y);
        let device_id = touch.device_id;
        let position = touch.location;
        let press = |state| WindowEvent::MouseInput { device_id, state, button: MouseButton::Left };
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

    /// An on-screen button: the key or mouse button it stands for.
    fn press(&mut self, control: Control, down: bool) {
        let result = match control {
            Control::Jump => self.mapped_key(KeyCode::Space, down, false),
            Control::Sneak => self.mapped_key(KeyCode::ShiftLeft, down, false),
            Control::Attack => self.play_mouse(MouseButton::Left, down),
            Control::Use => self.play_mouse(MouseButton::Right, down),
            // These open a screen: only the press matters, so a finger still
            // down when the screen opens leaves no key held in the world.
            Control::Inventory => self.key_pressed(KeyCode::KeyE),
            Control::Chat => self.key_pressed(KeyCode::KeyT),
            Control::Pause => self.key_pressed(KeyCode::Escape),
        };
        if let Err(e) = result {
            log::warn!("Horizon: on-screen {}: {e}", control.label());
        }
    }

    /// The window in physical pixels, as the renderer last saw it.
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

    /// Draw the controls over the HUD (Android, in the world).
    pub(super) fn draw_touch_controls(&mut self, batch: &mut HudBatch) {
        let layout = Layout::new((batch.width, batch.height));
        let atlas = &self.gui.atlas;
        let (sx, sy) = layout.stick;
        batch.fill(
            atlas,
            sx - STICK_RADIUS,
            sy - STICK_RADIUS,
            STICK_RADIUS * 2.0,
            STICK_RADIUS * 2.0,
            rgba(16, 16, 20, 90),
        );
        let knob = STICK_RADIUS * 0.45;
        let moving = self.touch_ui.stick != [0.0, 0.0];
        // The knob follows the finger: `stick` is `[left, forward]` and the
        // batch draws y down, so forward lifts the knob.
        batch.fill(
            atlas,
            sx - self.touch_ui.stick[0] * STICK_RADIUS - knob,
            sy - self.touch_ui.stick[1] * STICK_RADIUS - knob,
            knob * 2.0,
            knob * 2.0,
            rgba(255, 255, 255, if moving { 130 } else { 70 }),
        );
        for (control, (cx, cy), r) in &layout.buttons {
            let down = self.touch_ui.pressed.contains(control);
            batch.fill(
                atlas,
                cx - r,
                cy - r,
                r * 2.0,
                r * 2.0,
                rgba(16, 16, 20, 90),
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
                    rgba(70, 70, 80, 90)
                },
            );
            let label = control.label();
            let width = HudBatch::text_width(atlas, label);
            batch.text(
                atlas,
                label,
                (cx - width / 2.0).floor(),
                (cy - 4.0).floor(),
                rgba(255, 255, 255, 230),
                true,
            );
        }
    }
}

fn look_at(pos: (f64, f64)) -> Gesture {
    Gesture::Look { last: pos, travel: 0.0 }
}

/// The hotbar slot under `pos`, if the press landed on the hotbar.
fn hotbar_slot(pos: (f64, f64), (w, h): (f32, f32)) -> Option<usize> {
    let x0 = f64::from(w / 2.0 - HOTBAR_WIDTH / 2.0);
    let y0 = f64::from(h - 22.0);
    if pos.0 >= x0 && pos.0 < x0 + f64::from(HOTBAR_WIDTH) && pos.1 >= y0 && pos.1 <= f64::from(h) {
        Some((((pos.0 - x0) / f64::from(HOTBAR_SLOT)) as usize).min(8))
    } else {
        None
    }
}
