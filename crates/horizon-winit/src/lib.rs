//! A deliberately tiny compatibility layer around winit.
//!
//! Desktop receives the real crate unchanged. Android receives the same public
//! modules plus the one desktop-only extension Horizon's shared keybinding code
//! asks for. This keeps the upstream source file untouched.

#[cfg(not(target_os = "android"))]
pub use winit_real::*;

#[cfg(target_os = "android")]
pub use winit_real::{application, dpi, error, event, event_loop, keyboard, monitor, window};

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
