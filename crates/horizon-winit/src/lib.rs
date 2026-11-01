//! A deliberately tiny compatibility layer around winit.
//!
//! Desktop receives the real crate unchanged. Android receives the same public
//! modules plus the small platform adaptations shared Horizon needs: a
//! GameActivity-aware EventLoop constructor, modifier-supplement fallback, and
//! a touch/lifecycle adapter. Horizon's game loop remains upstream source.

#[cfg(not(target_os = "android"))]
pub use winit_real::*;

#[cfg(target_os = "android")]
pub use winit_real::{application, dpi, error, event, keyboard, monitor, window};

#[cfg(target_os = "android")]
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
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
    Look([f64; 2]),
    Button(u8),
    LeftTrigger,
    RightTrigger,
}

#[cfg(target_os = "android")]
#[derive(Default)]
struct AndroidTouch {
    state: AndroidTouchState,
    fingers: BTreeMap<u64, TouchRole>,
}

#[cfg(target_os = "android")]
static TOUCH: OnceLock<Mutex<AndroidTouch>> = OnceLock::new();
#[cfg(target_os = "android")]
static TOUCH_MENU: AtomicBool = AtomicBool::new(true);
#[cfg(target_os = "android")]
static WIDTH: AtomicU32 = AtomicU32::new(1);
#[cfg(target_os = "android")]
static HEIGHT: AtomicU32 = AtomicU32::new(1);

#[cfg(target_os = "android")]
fn touch() -> &'static Mutex<AndroidTouch> {
    TOUCH.get_or_init(|| Mutex::new(AndroidTouch::default()))
}

#[cfg(target_os = "android")]
pub fn android_touch_set_menu(menu: bool) {
    TOUCH_MENU.store(menu, Ordering::Relaxed);
    if menu {
        android_touch_clear();
    }
}

#[cfg(target_os = "android")]
pub fn android_touch_menu() -> bool {
    TOUCH_MENU.load(Ordering::Relaxed)
}

#[cfg(target_os = "android")]
pub fn android_touch_clear() {
    if let Ok(mut touch) = touch().lock() {
        *touch = AndroidTouch::default();
    }
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
                TouchRole::Look(_) => touch.state.right = [0.0; 2],
                TouchRole::Button(bit) => press_button(&mut touch.state, bit, false),
                TouchRole::LeftTrigger => touch.state.triggers[0] = 0.0,
                TouchRole::RightTrigger => touch.state.triggers[1] = 0.0,
            }
        }
        return;
    }

    if t.phase == TouchPhase::Started {
        // Geometry intentionally mirrors HorizonActivity.TouchOverlay. Sizes are
        // fractions of screen height so phones and tablets keep square controls.
        let role = if p[0] < 0.38 && p[1] > 0.34 {
            TouchRole::Move
        } else if near(p, [0.90, 0.74], 0.085, aspect) {
            TouchRole::Button(0) // South: jump
        } else if near(p, [0.79, 0.84], 0.065, aspect) {
            TouchRole::Button(11) // Right stick: sneak
        } else if near(p, [0.80, 0.58], 0.080, aspect) {
            TouchRole::RightTrigger // attack/mine
        } else if near(p, [0.92, 0.51], 0.075, aspect) {
            TouchRole::LeftTrigger // use/place
        } else if near(p, [0.72, 0.94], 0.055, aspect) {
            TouchRole::Button(3) // North: inventory
        } else if near(p, [0.50, 0.06], 0.055, aspect) {
            TouchRole::Button(9) // Start: pause
        } else {
            TouchRole::Look([t.location.x, t.location.y])
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
            // Fixed Bedrock-style joystick: centered at 14% x / 75% y.
            let radius = height * 0.16;
            let dx = (t.location.x - width * 0.14) / radius;
            let dy = (t.location.y - height * 0.75) / radius;
            let length = (dx * dx + dy * dy).sqrt().max(1.0);
            touch.state.left = [(dx / length) as f32, (-dy / length) as f32];
        }
        TouchRole::Look(previous) => {
            let dx = t.location.x - previous[0];
            let dy = t.location.y - previous[1];
            // Pad look is frame-rate integrated. Scale a finger delta into an
            // impulse and consume it on the next poll instead of leaving drift.
            touch.state.right = [
                (dx / (height * 0.025)).clamp(-1.0, 1.0) as f32,
                (-dy / (height * 0.025)).clamp(-1.0, 1.0) as f32,
            ];
            touch
                .fingers
                .insert(t.id, TouchRole::Look([t.location.x, t.location.y]));
        }
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
