//! Minimal Android entry point for Horizon.
//!
//! The game module below is the same source file the desktop binary uses.  The
//! only platform substitution here is `EventLoop::new`: winit requires the
//! AndroidApp handle to be attached to its builder on Android.
#![cfg(target_os = "android")]

use std::cell::RefCell;
use winit_real::platform::android::activity::AndroidApp;

thread_local! {
    static ANDROID_APP: RefCell<Option<AndroidApp>> = const { RefCell::new(None) };
}

fn android_app() -> AndroidApp {
    ANDROID_APP.with(|slot| {
        slot.borrow()
            .as_ref()
            .expect("android_main installs AndroidApp before Horizon starts")
            .clone()
    })
}

// Keep upstream game.rs untouched. Child modules resolve `winit::...` to this
// crate-local facade because the actual dependency is deliberately renamed to
// `winit_real`. Every type remains winit's real type; only EventLoop::new is
// adapted to Android's required builder extension.
mod winit {
    pub use winit_real::{application, dpi, error, event, keyboard, monitor, window};

    pub mod event_loop {
        pub use winit_real::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};

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
    }
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
    ANDROID_APP.with(|slot| *slot.borrow_mut() = Some(app));

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

    // Migrate Horizon's old single offline name into the profile store now.
    // The selected profile is mirrored back to horizon-player.txt, so existing
    // multiplayer code remains unchanged until the in-game account UI lands.
    let accounts = horizon_client::offline_accounts::Accounts::load_or_create(&home);
    log::info!("Offline profile: {}", accounts.selected_name());

    if let Err(error) = game::run() {
        log::error!("Horizon: {error}");
        horizon_client::crash::report_error(&home, "client", &*error);
    }
    horizon_client::logging::flush();
    ANDROID_APP.with(|slot| *slot.borrow_mut() = None);

    // Android may cache a GameActivity process. winit supports one EventLoop per
    // process, so after Horizon has fully exited we terminate the cached process.
    // App switching never reaches here: suspend/resume stays inside the event loop.
    std::process::exit(0);
}
