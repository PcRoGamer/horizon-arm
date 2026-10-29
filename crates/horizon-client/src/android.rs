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

/// Recreate NativeActivity on Android's UI thread. Its winit loop receives
/// Destroy and returns; android-activity then enters `android_main` again with a
/// fresh Activity, which reads `selected-instance` before rebuilding the game.
/// This is the launcherless equivalent of a launcher's "Play this instance".
pub fn recreate_activity() -> Result<(), String> {
    let app = app().ok_or_else(|| "Android Activity is unavailable".to_string())?;
    let queued = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        use jni::{JavaVM, jni_sig, jni_str, objects::JObject};

        let vm = unsafe { JavaVM::from_raw(queued.vm_as_ptr().cast()) };
        if let Err(error) = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            let raw_activity = queued.activity_as_ptr() as jni::sys::jobject;
            // android-activity owns this global reference; borrow it rather than
            // wrapping it in an owning Global/Auto reference.
            let activity = unsafe { env.as_cast_raw::<JObject>(&raw_activity)? };
            env.call_method(
                activity,
                jni_str!("recreate"),
                jni_sig!("()V"),
                &[],
            )?;
            Ok(())
        }) {
            log::error!("Horizon: could not recreate Android Activity: {error}");
        }
    }));
    Ok(())
}
