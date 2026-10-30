//! Android system haptics for Horizon touch controls.
//!
//! Unsafe JNI stays in the Android shell. `horizon-client` sees only a safe
//! function callback, preserving its crate-wide unsafe-code ban. We use
//! View.performHapticFeedback instead of driving the vibrator directly so the
//! device's accessibility / system haptic preferences remain authoritative.

use android_activity::AndroidApp;
use horizon_client::android::Haptic;

pub fn install() {
    horizon_client::android::set_haptic_hook(perform);
}

fn perform(kind: Haptic) {
    let Some(app) = horizon_client::android::app() else {
        return;
    };
    let ui_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        if let Err(error) = perform_on_ui(&ui_app, kind) {
            log::debug!("Horizon Android haptic unavailable: {error}");
        }
    }));
}

fn perform_on_ui(app: &AndroidApp, kind: Haptic) -> jni::errors::Result<()> {
    use jni::{jni_sig, jni_str, objects::{JObject, JValue}, refs::Global, vm::JavaVM};

    // AndroidApp owns both raw references for at least this closure. The cast is
    // borrowed/unowned: it must never delete NativeActivity's global reference.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
    vm.attach_current_thread(|env| {
        let raw_activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw_activity)? };
        let window = env
            .call_method(
                activity.as_ref(),
                jni_str!("getWindow"),
                jni_sig!("()Landroid/view/Window;"),
                &[],
            )?
            .l()?;
        if window.is_null() {
            return Ok(());
        }
        let view = env
            .call_method(
                &window,
                jni_str!("getDecorView"),
                jni_sig!("()Landroid/view/View;"),
                &[],
            )?
            .l()?;
        if view.is_null() {
            return Ok(());
        }

        // HapticFeedbackConstants: LONG_PRESS=0, VIRTUAL_KEY=1. Both exist on
        // every API Horizon supports (minSdk 26) and respect system settings.
        let constant = match kind {
            Haptic::Tap => 1,
            Haptic::Break => 0,
        };
        let _ = env.call_method(
            &view,
            jni_str!("performHapticFeedback"),
            jni_sig!("(I)Z"),
            &[JValue::Int(constant)],
        )?;
        Ok(())
    })
}
