//! Horizon on Android.
//!
//! Android starts `libhorizon_android.so` from `android.app.NativeActivity`.
//! The first launch can assemble Horizon's resources on-device: the APK carries
//! only Horizon-computed facts, while Minecraft's pinned jars are downloaded
//! directly from Mojang and checked before use. No Java runtime or login is
//! needed for that bootstrap.
//!
//! The Android shell is launcherless: after bootstrap it goes straight into the
//! last selected Horizon instance.  The original game folder remains the
//! default instance; additional instances have separate worlds, mods, packs and
//! options but share the one prepared Horizon runtime.
#![cfg(target_os = "android")]

mod bootstrap;
mod bundled;
mod instances;
mod loading;
mod smoke;

use std::path::{Path, PathBuf};

use android_activity::AndroidApp;

/// The activity: called by `android-activity`'s `ANativeActivity_onCreate`,
/// on the process's main thread, before any other Horizon thread exists.
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub extern "C" fn android_main(app: AndroidApp) {
    // `root` owns shared runtime/bootstrap data and the instance registry.  It
    // is deliberately not changed when a non-default instance is selected.
    let root = app
        .internal_data_path()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp/horizon"));
    let external = app.external_data_path().map(PathBuf::from);
    say(&format!("Horizon: data folder {}", root.display()));

    if let Err(e) = std::fs::create_dir_all(&root) {
        say(&format!("Horizon: cannot create {}: {e}", root.display()));
    }

    // A development/full APK may still carry a complete prepared folder.
    unpack_bundled(&app, &root);

    // A normal distributable APK carries only Horizon's computed facts. If no
    // resource folder exists yet, download the pinned Mojang jars and assemble
    // one now. App updates then reuse this shared folder.
    let mut resources = find_resources(&root, external.as_deref());
    if resources.is_none() {
        bootstrap_resources(&app, &root);
        resources = find_resources(&root, external.as_deref());
    }

    // Preserve the desktop launcher's instance invariant: existing players are
    // already the default instance; nothing gets moved just because instances
    // become available.
    if let Err(e) = instances::ensure(&root) {
        say(&format!("Horizon instances: {e}"));
    }
    let instance = instances::selected(&root);
    instances::follow_player(&root, &instance);
    let game_home = instance.home.clone();
    if let Err(e) = std::fs::create_dir_all(&game_home) {
        say(&format!("Horizon: cannot create {}: {e}", game_home.display()));
    }
    say(&format!(
        "Horizon: instance {} ({}) at {}",
        instance.name,
        instance.id,
        game_home.display()
    ));

    // Before anything reads them: the selected instance's game folder, its log,
    // and the shared prepared resources. `set_var` is unsound once another
    // thread reads the environment, and this runs before Horizon starts one.
    let world = std::fs::read_to_string(game_home.join("horizon-world"))
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty());
    unsafe {
        std::env::set_var("HORIZON_HOME", &game_home);
        std::env::set_var("HORIZON_ANDROID_ROOT", &root);
        std::env::set_var("HORIZON_INSTANCE_ID", &instance.id);
        std::env::set_var("HORIZON_INSTANCE_NAME", &instance.name);
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
    if let Err(e) = std::env::set_current_dir(&game_home) {
        say(&format!("Horizon: cannot enter {}: {e}", game_home.display()));
    }

    horizon_client::logging::init(&game_home, "Render thread");
    log::info!("Horizon {} (android)", horizon_client::crash::version());
    log::info!(
        "Android instance: {} ({})",
        instance.name,
        instance.id
    );
    horizon_client::crash::install(game_home.clone(), "client");

    if !horizon_client::android::set_app(app.clone()) {
        say("Horizon: the activity was already handed over");
        return;
    }

    let played_from = std::time::Instant::now();
    match &resources {
        Some(dir) => {
            say(&format!("Horizon: resources {}", dir.display()));
            match horizon_client::game::run() {
                Ok(()) => say("Horizon: the game closed"),
                Err(e) => {
                    log::error!("Horizon: {e}");
                    say(&format!("Horizon: {e}"));
                    horizon_client::crash::report_error(&game_home, "client", &*e);
                }
            }
        }
        None => {
            say("Horizon: first-run bootstrap failed; running the diagnostic screen");
            if let Err(e) = smoke::run(app) {
                log::error!("Horizon smoke: {e}");
                say(&format!("Horizon smoke: {e}"));
            }
        }
    }
    if let Err(e) = instances::record_play(&root, &instance.id, played_from.elapsed()) {
        log::warn!("Horizon: could not record instance play time: {e}");
    }
    horizon_client::logging::flush();
}

/// Downloads and assembles the first playable resource set in a worker while
/// the raw loading surface remains responsive. The game's winit event loop is
/// not created until this function returns.
fn bootstrap_resources(app: &AndroidApp, home: &Path) {
    if bootstrap::prepared(home) {
        return;
    }
    say("Horizon: first launch; downloading Minecraft 26.3 from Mojang");
    let progress = std::sync::Arc::new(bundled::Progress::default());
    let worker = {
        let progress = progress.clone();
        let app = app.clone();
        let home = home.to_path_buf();
        std::thread::spawn(move || {
            match bootstrap::run(&app, &home, progress.clone()) {
                Ok(dir) => say(&format!(
                    "Horizon: first-run resources ready at {}",
                    dir.display()
                )),
                Err(e) => {
                    say(&format!("Horizon bootstrap: {e}"));
                    if let Ok(mut slot) = progress.failed.lock() {
                        *slot = Some(e);
                    }
                }
            }
            progress
                .done
                .store(true, std::sync::atomic::Ordering::Release);
        })
    };
    if let Err(e) = loading::run(app, progress.clone()) {
        say(&format!("Horizon: no bootstrap loading screen: {e}"));
    }
    if worker.join().is_err() {
        say("Horizon bootstrap: worker panicked");
    }
}

/// Unpacks a full prepared folder when a development APK carries one.
fn unpack_bundled(app: &AndroidApp, home: &Path) {
    let manager = app.asset_manager();
    let version = horizon_core::VANILLA_VERSION;
    if !bundled::pending(&manager, home, version) {
        return;
    }
    say("Horizon: unpacking the data in the APK");
    let progress = std::sync::Arc::new(bundled::Progress::default());
    let worker = {
        let progress = progress.clone();
        let home = home.to_path_buf();
        std::thread::spawn(move || {
            let result = bundled::unpack(&manager, &home, version, &progress);
            match result {
                Ok(Some(done)) => say(&format!(
                    "Horizon: unpacked {} files ({} MiB) into {} in {:.1}s, {} not in the APK",
                    done.files,
                    done.bytes >> 20,
                    done.dir.display(),
                    done.secs,
                    done.missing
                )),
                Ok(None) => {}
                Err(e) => {
                    say(&format!("Horizon: cannot unpack the data in the APK: {e}"));
                    if let Ok(mut slot) = progress.failed.lock() {
                        *slot = Some(e.to_string());
                    }
                    progress
                        .done
                        .store(true, std::sync::atomic::Ordering::Release);
                }
            }
            progress
                .done
                .store(true, std::sync::atomic::Ordering::Release);
        })
    };
    if let Err(e) = loading::run(app, progress.clone()) {
        say(&format!("Horizon: no loading screen: {e}"));
    }
    let _ = worker.join();
}

/// Where the prepared resources are: `HORIZON_RESOURCES`, then the app's own
/// folders, then the shared external one an `adb push` can reach without root.
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
/// message here is written to liblog as well as printed.
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
    let Ok(text) = CString::new(message) else {
        return;
    };
    let tag = c"Horizon";
    unsafe {
        __android_log_write(4, tag.as_ptr(), text.as_ptr());
    }
}
