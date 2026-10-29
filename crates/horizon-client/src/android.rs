//! The Android activity this process runs in and Android-only game services.
//!
//! winit builds its event loop from the `AndroidApp` that `NativeActivity`
//! handed to `android_main` (`crates/horizon-android`). The handle is replaced
//! when Android recreates the Activity, which is also how launcherless instance
//! switching restarts Horizon without starting a second winit EventLoop.
use std::sync::Mutex;
use winit::platform::android::activity::AndroidApp;

pub mod content;
pub mod instances;

static APP: Mutex<Option<AndroidApp>> = Mutex::new(None);
static RECREATE: Mutex<Option<fn()>> = Mutex::new(None);

/// Hands the current Activity over before its event loop is built. Android may
/// legitimately destroy/recreate a NativeActivity in the same process, so this
/// must replace an older handle rather than be a process-lifetime OnceLock.
pub fn set_app(app: AndroidApp) -> bool {
    *APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(app);
    true
}

/// The currently live activity, when the wrapper handed one over.
pub fn app() -> Option<AndroidApp> {
    APP.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .cloned()
}

/// The Android shell owns JNI and registers this safe callback before the game
/// starts. Keeping the raw JNI bridge there lets horizon-client retain its
/// crate-wide `forbid(unsafe_code)` invariant.
pub fn set_recreate_hook(hook: fn()) {
    *RECREATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(hook);
}

/// Recreate NativeActivity on Android's UI thread. Its winit loop receives
/// Destroy and returns; android-activity then enters `android_main` again with a
/// fresh Activity, which reads `selected-instance` before rebuilding the game.
/// This is the launcherless equivalent of a launcher's "Play this instance".
pub fn recreate_activity() -> Result<(), String> {
    let hook = *RECREATE.lock().unwrap_or_else(|e| e.into_inner());
    hook.ok_or_else(|| "Android Activity restart hook is unavailable".to_string())?();
    Ok(())
}
