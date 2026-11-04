//! A deliberately tiny compatibility layer around winit.
//!
//! Desktop receives the real crate unchanged. Android receives the same public
//! modules plus the small platform adaptations shared Horizon needs: a
//! GameActivity-aware EventLoop constructor, modifier-supplement fallback, and
//! a touch/lifecycle adapter. Horizon's game loop remains upstream source.

#[cfg(any(test, target_os = "android"))]
mod touch_gestures;
#[cfg(target_os = "android")]
pub use touch_gestures::Action as AndroidGesture;

#[cfg(not(target_os = "android"))]
pub use winit_real::*;

#[cfg(target_os = "android")]
pub use winit_real::{application, dpi, error, event, keyboard, monitor, window};

#[cfg(target_os = "android")]
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

#[cfg(target_os = "android")]
thread_local! {
    static ANDROID_APP: RefCell<Option<winit_real::platform::android::activity::AndroidApp>> = const { RefCell::new(None) };
}

/// Installs the GameActivity handle before any shared Horizon code constructs
/// an event loop. Android may recreate the Activity; replacing the stored handle
/// before Horizon starts is intentional.
#[cfg(target_os = "android")]
pub fn install_android_app(app: winit_real::platform::android::activity::AndroidApp) {
    ANDROID_APP.with(|slot| *slot.borrow_mut() = Some(app));
}

#[cfg(target_os = "android")]
fn android_app() -> winit_real::platform::android::activity::AndroidApp {
    ANDROID_APP.with(|slot| {
        slot.borrow()
            .as_ref()
            .expect("AndroidApp must be installed before EventLoop::new")
            .clone()
    })
}

/// Controller-shaped state produced by Android touch. Button bits deliberately
/// match `horizon_client_input::gamepad::Button`'s declaration order without
/// making this low-level winit shim depend back on Horizon's input crate.
#[cfg(target_os = "android")]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AndroidTouchState {
    pub buttons: u32,
    pub left: [f32; 2],
    pub right: [f32; 2],
    pub triggers: [f32; 2],
}

#[cfg(target_os = "android")]
#[derive(Clone, Copy)]
enum TouchRole {
    Move,
    Look,
    Hotbar,
    Ignored,
    Button(u8),
    LeftTrigger,
    RightTrigger,
}

#[cfg(target_os = "android")]
struct AndroidTouch {
    state: AndroidTouchState,
    fingers: BTreeMap<u64, TouchRole>,
    gestures: touch_gestures::Recognizer,
    pending: VecDeque<AndroidGesture>,
    target: Option<([f32; 2], Instant)>,
}

#[cfg(target_os = "android")]
impl Default for AndroidTouch {
    fn default() -> Self {
        Self {
            state: AndroidTouchState::default(),
            fingers: BTreeMap::new(),
            gestures: touch_gestures::Recognizer::new(HEIGHT.load(Ordering::Relaxed) as f32),
            pending: VecDeque::new(),
            target: None,
        }
    }
}

#[cfg(target_os = "android")]
static TOUCH: OnceLock<Mutex<AndroidTouch>> = OnceLock::new();
#[cfg(target_os = "android")]
static TOUCH_MENU: AtomicBool = AtomicBool::new(true);
#[cfg(target_os = "android")]
static TOUCH_SCHEME: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "android")]
static TOUCH_SCALE: AtomicU32 = AtomicU32::new(100);
#[cfg(target_os = "android")]
static TOUCH_START: OnceLock<Instant> = OnceLock::new();
#[cfg(target_os = "android")]
static WIDTH: AtomicU32 = AtomicU32::new(1);
#[cfg(target_os = "android")]
static HEIGHT: AtomicU32 = AtomicU32::new(1);
#[cfg(target_os = "android")]
static GUI_WIDTH: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "android")]
static GUI_HEIGHT: AtomicU32 = AtomicU32::new(0);

/// The upstream HUD provides virtual GUI pixels after applying GUI Scale.
#[cfg(target_os = "android")]
pub fn android_touch_gui_size(width: u32, height: u32) {
    GUI_WIDTH.store(width, Ordering::Relaxed);
    GUI_HEIGHT.store(height, Ordering::Relaxed);
}

#[cfg(target_os = "android")]
fn hotbar_slot(p: [f64; 2]) -> Option<usize> {
    touch_gestures::hotbar_slot(
        [p[0] as f32, p[1] as f32],
        [
            GUI_WIDTH.load(Ordering::Relaxed),
            GUI_HEIGHT.load(Ordering::Relaxed),
        ],
    )
}

#[cfg(target_os = "android")]
fn touch() -> &'static Mutex<AndroidTouch> {
    TOUCH.get_or_init(|| Mutex::new(AndroidTouch::default()))
}

#[cfg(target_os = "android")]
pub fn android_touch_set_menu(menu: bool) {
    let was_menu = TOUCH_MENU.swap(menu, Ordering::Relaxed);
    if menu && !was_menu {
        android_touch_clear();
    }
}

#[cfg(target_os = "android")]
pub fn android_touch_set_controls(scheme: u32, scale: u32) {
    let scheme = scheme.min(3);
    let scale = scale.clamp(80, 120);
    let previous = TOUCH_SCHEME.swap(scheme, Ordering::Relaxed);
    let previous_scale = TOUCH_SCALE.swap(scale, Ordering::Relaxed);
    if previous != scheme || previous_scale != scale {
        android_touch_clear();
    }
}

#[cfg(target_os = "android")]
pub fn android_touch_crosshair() -> bool {
    TOUCH_SCHEME.load(Ordering::Relaxed) & 1 != 0
}

#[cfg(target_os = "android")]
fn android_touch_dpad() -> bool {
    TOUCH_SCHEME.load(Ordering::Relaxed) >= 2
}

#[cfg(target_os = "android")]
fn android_touch_scale() -> f64 {
    f64::from(TOUCH_SCALE.load(Ordering::Relaxed)) / 100.0
}

/// Overlay reads the same held input state that feeds the gamepad.
#[cfg(target_os = "android")]
pub fn android_touch_axes() -> [f32; 2] {
    touch()
        .lock()
        .map(|touch| touch.state.left)
        .unwrap_or_default()
}

#[cfg(target_os = "android")]
pub fn android_touch_button_bits() -> u32 {
    touch()
        .lock()
        .map(|touch| {
            touch.state.buttons
                | (u32::from(touch.state.triggers[0] > 0.5) << 24)
                | (u32::from(touch.state.triggers[1] > 0.5) << 25)
        })
        .unwrap_or_default()
}

#[cfg(target_os = "android")]
fn milliseconds() -> u64 {
    TOUCH_START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

#[cfg(target_os = "android")]
pub fn android_touch_menu() -> bool {
    TOUCH_MENU.load(Ordering::Relaxed)
}

#[cfg(target_os = "android")]
pub fn android_touch_clear() {
    if let Ok(mut touch) = touch().lock() {
        let release = touch.gestures.reset();
        *touch = AndroidTouch::default();
        if let Some(release) = release {
            touch.pending.push_back(release);
        }
    }
}

/// Consume tap/hold events once. Hold detection is performed here on every
/// game frame, including frames with no Android motion callbacks.
#[cfg(target_os = "android")]
pub fn android_touch_gestures() -> Vec<AndroidGesture> {
    let Ok(mut touch) = touch().lock() else {
        return Vec::new();
    };
    if !TOUCH_MENU.load(Ordering::Relaxed) {
        if let Some(action) = touch.gestures.tick(milliseconds()) {
            if !android_touch_crosshair() {
                if let AndroidGesture::HoldBegin { x, y } = action {
                    touch.target = Some(([x, y], Instant::now() + Duration::from_secs(3600)));
                    touch.pending.push_back(action);
                }
            }
        }
    }
    touch.pending.drain(..).collect()
}

/// The active touch target in fractional screen coordinates, plus aspect.
#[cfg(target_os = "android")]
pub fn android_touch_target() -> Option<([f64; 2], f64)> {
    if android_touch_crosshair() || TOUCH_MENU.load(Ordering::Relaxed) {
        return None;
    }
    let touch = touch().lock().ok()?;
    let (pos, until) = touch.target?;
    if Instant::now() > until {
        return None;
    }
    let w = WIDTH.load(Ordering::Relaxed).max(1) as f64;
    let h = HEIGHT.load(Ordering::Relaxed).max(1) as f64;
    Some(([f64::from(pos[0]) / w, f64::from(pos[1]) / h], w / h))
}

/// Read touch like a virtual controller. Look deltas are one-frame impulses;
/// held movement/buttons remain until the owning finger is released.
#[cfg(target_os = "android")]
pub fn android_touch_state() -> AndroidTouchState {
    let Ok(mut touch) = touch().lock() else {
        return AndroidTouchState::default();
    };
    let state = touch.state;
    touch.state.right = [0.0; 2];
    state
}

#[cfg(target_os = "android")]
fn norm(position: winit_real::dpi::PhysicalPosition<f64>) -> ([f64; 2], f64, f64) {
    let width = f64::from(WIDTH.load(Ordering::Relaxed).max(1));
    let height = f64::from(HEIGHT.load(Ordering::Relaxed).max(1));
    ([position.x / width, position.y / height], width, height)
}

#[cfg(target_os = "android")]
fn near(p: [f64; 2], center: [f64; 2], radius: f64, aspect: f64) -> bool {
    let dx = (p[0] - center[0]) * aspect;
    let dy = p[1] - center[1];
    dx * dx + dy * dy <= radius * radius
}

#[cfg(target_os = "android")]
fn near_square(p: [f64; 2], center: [f64; 2], radius: f64, aspect: f64) -> bool {
    ((p[0] - center[0]) * aspect).abs() <= radius && (p[1] - center[1]).abs() <= radius
}

#[cfg(target_os = "android")]
fn press_button(state: &mut AndroidTouchState, bit: u8, down: bool) {
    let mask = 1u32 << bit;
    if down {
        state.buttons |= mask;
    } else {
        state.buttons &= !mask;
    }
}

#[cfg(target_os = "android")]
fn handle_game_touch(t: winit_real::event::Touch) {
    use winit_real::event::TouchPhase;
    let (p, width, height) = norm(t.location);
    let aspect = width / height.max(1.0);
    let Ok(mut touch) = touch().lock() else {
        return;
    };

    if matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
        if let Some(role) = touch.fingers.remove(&t.id) {
            match role {
                TouchRole::Move => touch.state.left = [0.0; 2],
                TouchRole::Look => {
                    let action = touch.gestures.up(
                        t.id,
                        t.location.x as f32,
                        t.location.y as f32,
                        t.phase == TouchPhase::Cancelled,
                    );
                    if let Some(action) = action {
                        if !android_touch_crosshair() {
                            match action {
                                AndroidGesture::Tap { x, y } => {
                                    touch.target =
                                        Some(([x, y], Instant::now() + Duration::from_millis(300)));
                                    touch.pending.push_back(action);
                                }
                                AndroidGesture::HoldEnd => {
                                    touch.target = None;
                                    touch.pending.push_back(action);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                TouchRole::Hotbar => {
                    if t.phase != TouchPhase::Cancelled {
                        if let Some(slot) = hotbar_slot(p) {
                            touch.pending.push_back(AndroidGesture::Hotbar(slot));
                        }
                    }
                }
                TouchRole::Ignored => {}
                TouchRole::Button(bit) => {
                    // Two fingers may press the same control at once. Only the
                    // last release should clear its held gamepad button.
                    if !touch
                        .fingers
                        .values()
                        .any(|r| matches!(r, TouchRole::Button(other) if *other == bit))
                    {
                        press_button(&mut touch.state, bit, false);
                    }
                }
                TouchRole::LeftTrigger => {
                    if !touch
                        .fingers
                        .values()
                        .any(|r| matches!(r, TouchRole::LeftTrigger))
                    {
                        touch.state.triggers[0] = 0.0;
                    }
                }
                TouchRole::RightTrigger => {
                    if !touch
                        .fingers
                        .values()
                        .any(|r| matches!(r, TouchRole::RightTrigger))
                    {
                        touch.state.triggers[1] = 0.0;
                    }
                }
            }
        }
        return;
    }

    if t.phase == TouchPhase::Started {
        // Geometry intentionally mirrors HorizonActivity.TouchOverlay. Sizes are
        // fractions of screen height so phones and tablets keep square controls.
        let scale = android_touch_scale();
        let dpad = android_touch_dpad();
        let move_x = if dpad {
            0.11_f64.max(0.205 / aspect)
        } else {
            0.11
        };
        let gui_scale = (width / 320.0)
            .floor()
            .min((height / 240.0).floor())
            .max(1.0);
        let inventory_x = (0.735 * width).max(
            0.5 * width + 91.0 * gui_scale + 0.030 * height * scale + (0.015 * height).max(14.0),
        );
        let movement_owned = touch.fingers.values().any(|r| matches!(r, TouchRole::Move));
        let role = if hotbar_slot(p).is_some() {
            TouchRole::Hotbar
        } else if near_square(p, [0.11 + 0.190 / aspect, 0.62], 0.035 * scale, aspect) {
            TouchRole::Button(10) // Sprint wins over overlapping D-pad movement.
        } else if near(
            p,
            [move_x, 0.718],
            (if dpad { 0.160 } else { 0.112 }) * scale,
            aspect,
        ) {
            if movement_owned {
                TouchRole::Ignored
            } else {
                TouchRole::Move
            }
        } else if near_square(p, [1.0 - 0.160 / aspect, 0.77], 0.051 * scale, aspect) {
            TouchRole::Button(0) // South: jump
        } else if near_square(p, [1.0 - 0.290 / aspect, 0.77], 0.040 * scale, aspect) {
            TouchRole::Button(11) // Right stick: sneak
        } else if android_touch_crosshair()
            && near_square(p, [1.0 - 0.270 / aspect, 0.56], 0.042 * scale, aspect)
        {
            TouchRole::RightTrigger // attack/mine
        } else if android_touch_crosshair()
            && near_square(p, [1.0 - 0.145 / aspect, 0.52], 0.042 * scale, aspect)
        {
            TouchRole::LeftTrigger // use/place
        } else if near_square(p, [inventory_x / width, 0.92], 0.030 * scale, aspect) {
            TouchRole::Button(3) // North: inventory
        } else if near_square(p, [0.50, 0.072], 0.032 * scale, aspect) {
            TouchRole::Button(9) // Start: pause
        } else if touch.gestures.down(
            t.id,
            t.location.x as f32,
            t.location.y as f32,
            milliseconds(),
        ) {
            TouchRole::Look
        } else {
            TouchRole::Ignored
        };
        match role {
            TouchRole::Button(bit) => press_button(&mut touch.state, bit, true),
            TouchRole::LeftTrigger => touch.state.triggers[0] = 1.0,
            TouchRole::RightTrigger => touch.state.triggers[1] = 1.0,
            _ => {}
        }
        touch.fingers.insert(t.id, role);
    }

    let Some(role) = touch.fingers.get(&t.id).copied() else {
        return;
    };
    match role {
        TouchRole::Move => {
            // The drawn thumb and movement physics share a center and scale.
            let radius =
                height * (if android_touch_dpad() { 0.110 } else { 0.092 }) * android_touch_scale();
            let center_x = if android_touch_dpad() {
                (width * 0.11).max(height * 0.205)
            } else {
                width * 0.11
            };
            let dx = (t.location.x - center_x) / radius;
            let dy = (t.location.y - height * 0.718) / radius;
            if android_touch_dpad() {
                // An 8-way D-pad with a neutral center and intentional diagonal input.
                let digital = |axis: f64| {
                    if axis > 0.28 {
                        1.0
                    } else if axis < -0.28 {
                        -1.0
                    } else {
                        0.0
                    }
                };
                touch.state.left = [digital(dx) as f32, -digital(dy) as f32];
            } else {
                let length = (dx * dx + dy * dy).sqrt().max(1.0);
                touch.state.left = [(dx / length) as f32, (-dy / length) as f32];
            }
        }
        TouchRole::Look => {
            if let Some(AndroidGesture::Look { dx, dy }) =
                touch
                    .gestures
                    .move_to(t.id, t.location.x as f32, t.location.y as f32)
            {
                // Deltas are accumulated, never replaced, between game frames.
                touch.state.right[0] =
                    (touch.state.right[0] + dx / (height as f32 * 0.025)).clamp(-1.0, 1.0);
                touch.state.right[1] =
                    (touch.state.right[1] - dy / (height as f32 * 0.025)).clamp(-1.0, 1.0);
                touch.target = None;
            }
        }
        TouchRole::Hotbar | TouchRole::Ignored => {}
        _ => {}
    }
}

#[cfg(target_os = "android")]
pub mod event_loop {
    pub use winit_real::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoopProxy};

    /// Same type surface Horizon uses from winit, with only Android construction
    /// and event adaptation added around the real event loop.
    pub struct EventLoop<T: 'static>(winit_real::event_loop::EventLoop<T>);

    impl EventLoop<()> {
        pub fn new() -> Result<Self, winit_real::error::EventLoopError> {
            use winit_real::platform::android::EventLoopBuilderExtAndroid;
            let mut builder = winit_real::event_loop::EventLoop::builder();
            builder.with_android_app(crate::android_app());
            builder.build().map(Self)
        }
    }

    struct AndroidHandler<'a, A>(&'a mut A);

    impl<T: 'static, A: winit_real::application::ApplicationHandler<T>>
        winit_real::application::ApplicationHandler<T> for AndroidHandler<'_, A>
    {
        fn new_events(
            &mut self,
            event_loop: &winit_real::event_loop::ActiveEventLoop,
            cause: winit_real::event::StartCause,
        ) {
            self.0.new_events(event_loop, cause);
        }

        fn resumed(&mut self, event_loop: &winit_real::event_loop::ActiveEventLoop) {
            crate::android_touch_clear();
            self.0.resumed(event_loop);
        }

        fn user_event(&mut self, event_loop: &winit_real::event_loop::ActiveEventLoop, event: T) {
            self.0.user_event(event_loop, event);
        }

        fn window_event(
            &mut self,
            event_loop: &winit_real::event_loop::ActiveEventLoop,
            window_id: winit_real::window::WindowId,
            event: winit_real::event::WindowEvent,
        ) {
            use winit_real::event::{ElementState, MouseButton, TouchPhase, WindowEvent};
            match event {
                WindowEvent::Resized(size) => {
                    crate::WIDTH.store(size.width.max(1), std::sync::atomic::Ordering::Relaxed);
                    crate::HEIGHT.store(size.height.max(1), std::sync::atomic::Ordering::Relaxed);
                    if let Ok(mut touch) = crate::touch().lock() {
                        touch.gestures.set_screen_height(size.height as f32);
                    }
                    self.0
                        .window_event(event_loop, window_id, WindowEvent::Resized(size));
                }
                WindowEvent::Focused(false) => {
                    crate::android_touch_clear();
                    self.0
                        .window_event(event_loop, window_id, WindowEvent::Focused(false));
                }
                WindowEvent::Touch(touch) if crate::android_touch_menu() => {
                    self.0.window_event(
                        event_loop,
                        window_id,
                        WindowEvent::CursorMoved {
                            device_id: touch.device_id,
                            position: touch.location,
                        },
                    );
                    let state = match touch.phase {
                        TouchPhase::Started => Some(ElementState::Pressed),
                        TouchPhase::Ended | TouchPhase::Cancelled => Some(ElementState::Released),
                        TouchPhase::Moved => None,
                    };
                    if let Some(state) = state {
                        self.0.window_event(
                            event_loop,
                            window_id,
                            WindowEvent::MouseInput {
                                device_id: touch.device_id,
                                state,
                                button: MouseButton::Left,
                            },
                        );
                    }
                }
                WindowEvent::Touch(touch) => crate::handle_game_touch(touch),
                other => self.0.window_event(event_loop, window_id, other),
            }
        }

        fn device_event(
            &mut self,
            event_loop: &winit_real::event_loop::ActiveEventLoop,
            device_id: winit_real::event::DeviceId,
            event: winit_real::event::DeviceEvent,
        ) {
            self.0.device_event(event_loop, device_id, event);
        }

        fn about_to_wait(&mut self, event_loop: &winit_real::event_loop::ActiveEventLoop) {
            self.0.about_to_wait(event_loop);
        }

        fn suspended(&mut self, event_loop: &winit_real::event_loop::ActiveEventLoop) {
            crate::android_touch_clear();
            self.0.suspended(event_loop);
        }

        fn exiting(&mut self, event_loop: &winit_real::event_loop::ActiveEventLoop) {
            crate::android_touch_clear();
            self.0.exiting(event_loop);
        }

        fn memory_warning(&mut self, event_loop: &winit_real::event_loop::ActiveEventLoop) {
            self.0.memory_warning(event_loop);
        }
    }

    impl<T: 'static> EventLoop<T> {
        pub fn run_app<A: winit_real::application::ApplicationHandler<T>>(
            self,
            app: &mut A,
        ) -> Result<(), winit_real::error::EventLoopError> {
            self.0.run_app(&mut AndroidHandler(app))
        }
    }

    impl<T: 'static> std::ops::Deref for EventLoop<T> {
        type Target = winit_real::event_loop::EventLoop<T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
}

#[cfg(target_os = "android")]
pub mod platform {
    pub use winit_real::platform::*;

    /// winit exposes this helper on desktop backends only. Android's physical
    /// keyboard events already carry a logical key, while soft-keyboard text is
    /// handled by GameActivity's IME buffer, so the unmodified logical key is
    /// the correct fallback for learning a display label.
    pub mod modifier_supplement {
        use winit_real::{event::KeyEvent, keyboard::Key};

        pub trait KeyEventExtModifierSupplement {
            fn key_without_modifiers(&self) -> Key;
        }

        impl KeyEventExtModifierSupplement for KeyEvent {
            fn key_without_modifiers(&self) -> Key {
                self.logical_key.clone()
            }
        }
    }
}
