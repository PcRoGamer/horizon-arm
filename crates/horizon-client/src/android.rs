//! The Android activity this process runs in and Android-only game services.
//!
//! winit builds its event loop from the `AndroidApp` that `NativeActivity`
//! handed to `android_main` (`crates/horizon-android`). That wrapper runs on
//! the main thread before anything else, stores it here, and the game loop
//! asks for it when it builds its event loop.
use std::sync::OnceLock;
use winit::platform::android::activity::AndroidApp;

pub mod instances;

static APP: OnceLock<AndroidApp> = OnceLock::new();

/// Hands the activity over, once, before the event loop is built.
pub fn set_app(app: AndroidApp) -> bool {
    APP.set(app).is_ok()
}

/// The activity, when the wrapper handed it over.
pub fn app() -> Option<AndroidApp> {
    APP.get().cloned()
}
