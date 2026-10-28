//! Horizon on Android.
//!
//! Android starts `libhorizon_android.so` from `android.app.NativeActivity`
//! (no Java code in the APK: `AndroidManifest.xml` names the framework's own
//! activity and the library through `android.app.lib_name`). `android_main`
//! below is the whole entry point: it points the game at the app's own folders,
//! hands the activity to `horizon_client::android` for winit, and runs the game.
//!
//! Without prepared resources (`horizon-prepare` on a desktop, then pushed to
//! the device) the game cannot draw a title screen, so this falls back to a
//! smoke screen: a window, a wgpu adapter, a cleared frame, and in logcat what
//! the device offers. That is what a port is proved with before any Mojang data
//! is on the phone.
#![cfg(target_os = "android")]

mod smoke;

use std::path::{Path, PathBuf};

use android_activity::AndroidApp;

/// The activity: called by `android-activity`'s `ANativeActivity_onCreate`,
/// on the process's main thread, before any other Horizon thread exists.
// `AndroidApp` is a handle to the activity the platform owns; `android-activity`
// defines this ABI (same signature, same symbol name) and calls it.
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub extern "C" fn android_main(app: AndroidApp) {
    let home = app
        .internal_data_path()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp/horizon"));
    let external = app.external_data_path().map(PathBuf::from);
    say(&format!("Horizon: data folder {}", home.display()));

    // Before anything reads them: the game's folders, its log, and where the
    // prepared resources are. `set_var` is unsound once another thread reads the
    // environment, and this runs before the first Horizon thread starts.
    let resources = find_resources(&home, external.as_deref());
    // A `horizon-world` file next to the log is the way in without a command
    // line: its name opens that world (created on first run), as `--world` does.
    let world = std::fs::read_to_string(home.join("horizon-world"))
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty());
    unsafe {
        std::env::set_var("HORIZON_HOME", &home);
        if let Some(dir) = &resources {
            std::env::set_var("HORIZON_RESOURCES", dir);
        }
        if let Some(name) = &world {
            std::env::set_var("HORIZON_WORLD", name);
        }
        // Nothing on a phone speaks Discord's IPC; the game would keep a thread
        // retrying a socket that cannot exist.
        std::env::set_var("HORIZON_NO_DISCORD", "1");
    }
    if let Err(e) = std::fs::create_dir_all(&home) {
        say(&format!("Horizon: cannot create {}: {e}", home.display()));
    }
    if let Err(e) = std::env::set_current_dir(&home) {
        say(&format!("Horizon: cannot enter {}: {e}", home.display()));
    }

    horizon_client::logging::init(&home, "Render thread");
    log::info!("Horizon {} (android)", horizon_client::crash::version());
    horizon_client::crash::install(home.clone(), "client");

    if !horizon_client::android::set_app(app.clone()) {
        say("Horizon: the activity was already handed over");
        return;
    }

    match &resources {
        Some(dir) => {
            say(&format!("Horizon: resources {}", dir.display()));
            match horizon_client::game::run() {
                Ok(()) => say("Horizon: the game closed"),
                Err(e) => {
                    log::error!("Horizon: {e}");
                    say(&format!("Horizon: {e}"));
                    horizon_client::crash::report_error(&home, "client", &*e);
                }
            }
        }
        None => {
            say("Horizon: no prepared resources found; running the smoke screen");
            say("Horizon: push a prepared 26.3 folder to the paths above to play");
            if let Err(e) = smoke::run(app) {
                log::error!("Horizon smoke: {e}");
                say(&format!("Horizon smoke: {e}"));
            }
        }
    }
    horizon_client::logging::flush();
}

/// Where the prepared resources are: `HORIZON_RESOURCES`, then the app's own
/// folders, then the shared external one an `adb push` can reach without root.
/// A prepared folder carries `horizon-resources.json` (`horizon-prepare`).
fn find_resources(home: &Path, external: Option<&Path>) -> Option<PathBuf> {
    let version = horizon_core::VANILLA_VERSION;
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("HORIZON_RESOURCES").filter(|v| !v.is_empty()) {
        candidates.push(PathBuf::from(dir));
    }
    for root in std::iter::once(home).chain(external) {
        candidates.push(root.join(".local/horizon-client").join(version));
        candidates.push(root.join("horizon-client").join(version));
        candidates.push(root.join("resources"));
    }
    candidates
        .into_iter()
        .find(|dir| dir.join("horizon-resources.json").is_file())
}

/// logcat: an app's standard output goes to `/dev/null` on Android, so a
/// message here is written to liblog as well as printed. Before the game's own
/// log file opens, and for anyone watching `adb logcat`, this is the record of
/// what the port found on the device.
fn say(message: &str) {
    println!("{message}");
    logcat(message);
}

/// `android/log.h`: `ANDROID_LOG_INFO`, tag "Horizon".
fn logcat(message: &str) {
    use std::ffi::{CString, c_char, c_int};
    unsafe extern "C" {
        fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }
    let Ok(text) = CString::new(message) else { return };
    let tag = c"Horizon";
    // Safety: both are NUL-terminated strings that outlive the call.
    unsafe {
        __android_log_write(4, tag.as_ptr(), text.as_ptr());
    }
}
