//! Minimal Android entry point for Horizon.
//!
//! The game module below is the same source file the desktop binary uses. All
//! Android-specific event-loop construction lives in the additive `horizon-winit`
//! compatibility crate, so `game.rs` and its child modules remain untouched.
#![cfg(target_os = "android")]

use std::cell::RefCell;
use winit::platform::android::activity::AndroidApp;

#[path = "../../horizon-android/src/bootstrap.rs"]
mod bootstrap;

thread_local! {
    /// Kept for the Android IME/clipboard/haptics bridge added by the platform
    /// layer. `horizon-winit` stores its own clone for EventLoop construction.
    pub(crate) static ANDROID_APP: RefCell<Option<AndroidApp>> = const { RefCell::new(None) };
}

mod game;

#[global_allocator]
static ALLOCATOR: horizon_threads::Allocator = horizon_threads::Allocator;

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("Horizon"),
    );
    horizon_threads::configure_allocator();

    let Some(home) = app.internal_data_path() else {
        log::error!("Android supplied no application data directory");
        return;
    };

    // Winit requires AndroidApp on its EventLoopBuilder. Installing it here lets
    // untouched upstream code continue to call EventLoop::new().
    winit::install_android_app(app.clone());
    ANDROID_APP.with(|slot| *slot.borrow_mut() = Some(app.clone()));

    if let Err(error) =
        std::fs::create_dir_all(&home).and_then(|()| std::env::set_current_dir(&home))
    {
        log::error!(
            "Cannot use Android application directory {}: {error}",
            home.display()
        );
        return;
    }

    horizon_client::logging::set_game_home(home.clone());
    horizon_client::logging::init(&home, "Render thread");
    horizon_client::crash::install(home.clone(), "client");

    // The APK carries Horizon-generated engine tables only. Minecraft's client
    // resources and indexed assets are fetched directly from Mojang, verified
    // against the repository's pinned version, and installed atomically into
    // the same path the untouched desktop game loop already uses.
    if let Err(error) = bootstrap::ensure(&home, &app) {
        log::error!("Minecraft resource bootstrap failed: {error}");
        horizon_client::crash::report_error(&home, "resource-bootstrap", &*error);
        horizon_client::logging::flush();
        return;
    }

    // Migrate Horizon's old single offline name into the profile store now.
    // The selected profile is mirrored back to horizon-player.txt, so existing
    // multiplayer code remains unchanged while the in-game Profile UI is added.
    let accounts = horizon_client::offline_accounts::Accounts::load_or_create(&home);
    log::info!("Offline profile: {}", accounts.selected_name());

    if let Err(error) = game::run() {
        log::error!("Horizon: {error}");
        horizon_client::crash::report_error(&home, "client", &*error);
    }
    horizon_client::logging::flush();
    ANDROID_APP.with(|slot| *slot.borrow_mut() = None);

    // Android may cache a GameActivity process. winit supports one EventLoop per
    // process, so terminate only after Horizon genuinely exits. App switching
    // stays inside ApplicationHandler suspend/resume and never reaches here.
    std::process::exit(0);
}
