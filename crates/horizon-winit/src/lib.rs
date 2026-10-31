//! A deliberately tiny compatibility layer around winit.
//!
//! Desktop receives the real crate unchanged. Android receives the same public
//! modules, plus the two pieces shared Horizon needs but upstream winit leaves
//! platform-specific: the GameActivity-aware EventLoop constructor and the
//! desktop-only modifier-supplement helper. Keeping both here leaves Horizon's
//! game and input source files untouched.

#[cfg(not(target_os = "android"))]
pub use winit_real::*;

#[cfg(target_os = "android")]
pub use winit_real::{application, dpi, error, event, keyboard, monitor, window};

#[cfg(target_os = "android")]
use std::cell::RefCell;

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

#[cfg(target_os = "android")]
pub mod event_loop {
    pub use winit_real::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoopProxy};

    /// Same type surface Horizon uses from winit, with only `new()` adapted to
    /// Android's mandatory GameActivity builder extension.
    pub struct EventLoop<T: 'static>(winit_real::event_loop::EventLoop<T>);

    impl EventLoop<()> {
        pub fn new() -> Result<Self, winit_real::error::EventLoopError> {
            use winit_real::platform::android::EventLoopBuilderExtAndroid;
            let mut builder = winit_real::event_loop::EventLoop::builder();
            builder.with_android_app(crate::android_app());
            builder.build().map(Self)
        }
    }

    impl<T: 'static> EventLoop<T> {
        pub fn run_app<A: winit_real::application::ApplicationHandler<T>>(
            self,
            app: &mut A,
        ) -> Result<(), winit_real::error::EventLoopError> {
            self.0.run_app(app)
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
