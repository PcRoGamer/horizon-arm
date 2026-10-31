//! Keep Horizon's existing `arboard` call sites unchanged while making the
//! dependency portable to Android. Desktop is a zero-behaviour-change re-export
//! of upstream arboard; Android uses callbacks installed by the GameActivity
//! shell, with an in-process fallback until the activity is available.

#[cfg(not(target_os = "android"))]
pub use arboard_real::{Clipboard, Error};

#[cfg(target_os = "android")]
mod android {
    use std::sync::{Mutex, OnceLock};

    pub type GetFn = fn() -> Option<String>;
    pub type SetFn = fn(&str) -> bool;

    #[derive(Clone, Copy)]
    pub struct Backend {
        pub get: GetFn,
        pub set: SetFn,
    }

    static BACKEND: OnceLock<Backend> = OnceLock::new();
    static FALLBACK: Mutex<Option<String>> = Mutex::new(None);

    /// Installs the OS clipboard bridge once. Repeated installation is harmless,
    /// which matters when Android recreates an Activity around the same process.
    pub fn install_backend(backend: Backend) {
        let _ = BACKEND.set(backend);
    }

    #[derive(Debug)]
    pub enum Error {
        ContentNotAvailable,
        ClipboardNotSupported,
    }

    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::ContentNotAvailable => f.write_str("clipboard has no text"),
                Self::ClipboardNotSupported => f.write_str("clipboard unavailable"),
            }
        }
    }

    impl std::error::Error for Error {}

    pub struct Clipboard;

    impl Clipboard {
        pub fn new() -> Result<Self, Error> {
            Ok(Self)
        }

        pub fn get_text(&mut self) -> Result<String, Error> {
            if let Some(backend) = BACKEND.get()
                && let Some(text) = (backend.get)()
            {
                return Ok(text);
            }
            FALLBACK
                .lock()
                .ok()
                .and_then(|text| text.clone())
                .ok_or(Error::ContentNotAvailable)
        }

        pub fn set_text(&mut self, text: String) -> Result<(), Error> {
            if let Ok(mut fallback) = FALLBACK.lock() {
                *fallback = Some(text.clone());
            }
            if let Some(backend) = BACKEND.get()
                && !(backend.set)(&text)
            {
                return Err(Error::ClipboardNotSupported);
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "android")]
pub use android::{Backend, Clipboard, Error, install_backend};
