//! Android touchscreen HUD.
//!
//! The interaction model follows the mature Bedrock/TouchController pattern:
//! independent fingers, a floating left movement stick, a large unobstructed
//! look surface, a right-thumb action cluster, direct hotbar selection and
//! normal Minecraft key/mouse actions underneath. Menus remain a plain pointer.
use super::{Game, Screen};
use horizon_client::hud::{HudBatch, rgba};
use std::{collections::HashMap, time::Instant};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, TouchPhase, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
    window::WindowId,
};

const STICK_RADIUS: f32 = 54.0;
const STICK_DEADZONE: f32 = 0.12;
const SLOP: f32 = 1.20;
const HOTBAR_WIDTH: f32 = 182.0;
const HOTBAR_SLOT: f32 = 20.0;

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
            Control::Jump => "JUMP",
            Control::Sneak => "SNEAK",
            Control::Attack => "HIT",
            Control::Use => "USE",
            Control::Inventory => "ITEMS",
            Control::Chat => "CHAT",
            Control::Pause => "MENU",
        }
    }
}

struct Layout {
    default_stick: (f32, f32),
    buttons: Vec<(Control, (f32, f32), f32)>,
    width: f32,
    height: f32,
}

impl Layout {
    fn new((w, h): (f32, f32)) -> Self {
        use Control::*;
        // Keep the centre/right of the display clear for looking. The four
        // gameplay actions form the same reachable thumb arc used by mature
        // Bedrock-style mobile layouts.
        Self {
            default_stick: (STICK_RADIUS + 18.0, h - STICK_RADIUS - 20.0),
            buttons: vec![
                (Jump, (w - 62.0, h - 62.0), 31.0),
                (Attack, (w - 142.0, h - 54.0), 25.0),
                (Use, (w - 54.0, h - 140.0), 25.0),
                (Sneak, (w - 137.0, h - 132.0), 24.0),
                (Inventory, (w - 34.0, 30.0), 21.0),
                (Chat, (w - 86.0, 30.0), 21.0),
                (Pause, (w - 138.0, 30.0), 21.0),
            ],
            width: w,
            height: h,
        }
    }

    fn button_at(&self, pos: (f64, f64)) -> Option<Control> {
        let hit = |(cx, cy): (f32, f32), r: f32| {
            let (dx, dy) = (pos.0 - f64::from(cx), pos.1 - f64::from(cy));
            dx * dx + dy * dy <= f64::from(r * SLOP).powi(2)
        };
        self.buttons.iter().rev().find(|(_, centre, r)| hit(*centre, *r)).map(|(c, _, _)| *c)
    }

    /// TouchController-style floating joystick activation region. The stick is
    /// centred where the movement thumb actually lands instead of forcing the
    /// player to hunt for one exact pixel position.
    fn on_stick(&self, pos: (f64, f64)) -> bool {
        pos.0 < f64::from(self.width * 0.46) && pos.1 > f64::from(self.height * 0.42)
    }
}

#[derive(Clone, Copy, Debug)]
enum Gesture {
    Look { last: (f64, f64), travel: f64 },
    Stick { origin: (f64, f64) },
    Button(Control),
    Hotbar,
}

/// Every finger has an independent role. The old port tracked one finger,
/// which meant looking or jumping cancelled movement on a real phone.
pub(super) struct Touch {
    fingers: HashMap<u64, Gesture>,
    stick: [f32; 2],
    stick_origin: Option<(f32, f32)>,
    pressed: Vec<Control>,
}

impl Default for Touch {
    fn default() -> Self {
        Self {
            fingers: HashMap::new(),
            stick: [0.0; 2],
            stick_origin: None,
            pressed: Vec::new(),
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
        let layout = Layout::new((batch.width, batch.height));
        let scale = f64::from(batch.scale);
        let pos = (touch.location.x / scale, touch.location.y / scale);
        let id = touch.id;

        match touch.phase {
            TouchPhase::Started => {
                // Capture immediately but do not throw away the first touch.
                if !self.captured {
                    self.capture(true);
                }

                let gesture = if let Some(control) = layout.button_at(pos) {
                    if !self.touch_ui.pressed.contains(&control) {
                        self.touch_ui.pressed.push(control);
                        self.press(control, true);
                    }
                    Gesture::Button(control)
                } else if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                    self.selected = slot;
                    Gesture::Hotbar
                } else if layout.on_stick(pos)
                    && !self.touch_ui.fingers.values().any(|g| matches!(g, Gesture::Stick { .. }))
                {
                    self.touch_ui.stick_origin = Some((pos.0 as f32, pos.1 as f32));
                    Gesture::Stick { origin: pos }
                } else {
                    look_at(pos)
                };
                self.touch_ui.fingers.insert(id, gesture);
            }
            TouchPhase::Moved => {
                // Temporarily remove the gesture so calls that mutably borrow
                // Game (mouse input) do not alias the touch map.
                let Some(mut gesture) = self.touch_ui.fingers.remove(&id) else { return; };
                match &mut gesture {
                    Gesture::Look { last, travel } => {
                        let (dx, dy) = (pos.0 - last.0, pos.1 - last.1);
                        *travel += (dx * dx + dy * dy).sqrt();
                        *last = pos;
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
                    Gesture::Button(_) | Gesture::Hotbar => {}
                }
                self.touch_ui.fingers.insert(id, gesture);
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let Some(gesture) = self.touch_ui.fingers.remove(&id) else { return; };
                match gesture {
                    Gesture::Look { travel, .. }
                        if touch.phase == TouchPhase::Ended && travel < 8.0 =>
                    {
                        self.touch_click(event_loop, window, touch);
                    }
                    Gesture::Look { .. } | Gesture::Hotbar => {}
                    Gesture::Stick { .. } => {
                        self.touch_ui.stick = [0.0; 2];
                        self.touch_ui.stick_origin = None;
                    }
                    Gesture::Button(control) => {
                        // Release only when no other finger is holding the same
                        // control (important for overlapping thumb transitions).
                        let still_down = self.touch_ui.fingers.values().any(
                            |g| matches!(g, Gesture::Button(c) if *c == control),
                        );
                        if !still_down {
                            self.touch_ui.pressed.retain(|c| *c != control);
                            self.press(control, false);
                        }
                    }
                }
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
        let press = |state| WindowEvent::MouseInput { device_id, state, button: MouseButton::Left };
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
            (TouchPhase::Moved | TouchPhase::Ended, Some((finger, last))) if finger == touch.id => Some(last),
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
                    winit::event::DeviceEvent::MouseMotion { delta: (x - last_x, y - last_y) },
                );
            }
        }
    }

    fn press(&mut self, control: Control, down: bool) {
        let result = match control {
            Control::Jump => self.mapped_key(KeyCode::Space, down, false),
            Control::Sneak => self.mapped_key(KeyCode::ShiftLeft, down, false),
            Control::Attack => self.play_mouse(MouseButton::Left, down),
            Control::Use => self.play_mouse(MouseButton::Right, down),
            Control::Inventory => self.key_pressed(KeyCode::KeyE),
            Control::Chat => self.key_pressed(KeyCode::KeyT),
            Control::Pause => self.key_pressed(KeyCode::Escape),
        };
        if let Err(e) = result {
            log::warn!("Horizon: on-screen {}: {e}", control.label());
        }
    }

    fn touch_pixels(&self) -> [u32; 2] {
        self.renderer.as_ref().map_or_else(
            || self.window.as_ref().map(|w| w.inner_size().into()).unwrap_or([1, 1]),
            |r| [r.config.width, r.config.height],
        )
    }

    /// Render a low-obstruction Bedrock-like HUD. The stick base moves to the
    /// thumb while active; action controls brighten independently per finger.
    pub(super) fn draw_touch_controls(&mut self, batch: &mut HudBatch) {
        let layout = Layout::new((batch.width, batch.height));
        let atlas = &self.gui.atlas;
        let (sx, sy) = self.touch_ui.stick_origin.unwrap_or(layout.default_stick);

        batch.fill(
            atlas,
            sx - STICK_RADIUS,
            sy - STICK_RADIUS,
            STICK_RADIUS * 2.0,
            STICK_RADIUS * 2.0,
            rgba(8, 8, 12, 58),
        );
        let knob = STICK_RADIUS * 0.42;
        let moving = self.touch_ui.stick != [0.0, 0.0];
        batch.fill(
            atlas,
            sx - self.touch_ui.stick[0] * STICK_RADIUS - knob,
            sy - self.touch_ui.stick[1] * STICK_RADIUS - knob,
            knob * 2.0,
            knob * 2.0,
            rgba(255, 255, 255, if moving { 145 } else { 72 }),
        );

        for (control, (cx, cy), r) in &layout.buttons {
            let down = self.touch_ui.pressed.contains(control);
            batch.fill(atlas, cx - r, cy - r, r * 2.0, r * 2.0, rgba(8, 8, 12, if down { 128 } else { 62 }));
            let inner = r - 2.0;
            batch.fill(
                atlas,
                cx - inner,
                cy - inner,
                inner * 2.0,
                inner * 2.0,
                if down { rgba(255, 255, 255, 88) } else { rgba(50, 50, 58, 58) },
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
    Gesture::Look { last: pos, travel: 0.0 }
}

fn hotbar_slot(pos: (f64, f64), (w, h): (f32, f32)) -> Option<usize> {
    let x0 = f64::from(w / 2.0 - HOTBAR_WIDTH / 2.0);
    let y0 = f64::from(h - 22.0);
    if pos.0 >= x0 && pos.0 < x0 + f64::from(HOTBAR_WIDTH) && pos.1 >= y0 && pos.1 <= f64::from(h) {
        Some((((pos.0 - x0) / f64::from(HOTBAR_SLOT)) as usize).min(8))
    } else {
        None
    }
}
