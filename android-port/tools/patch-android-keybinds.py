#!/usr/bin/env python3
"""Keep Horizon key-label learning portable on Android.

winit's KeyEventExtModifierSupplement is only exposed on desktop backends.
Desktop keeps upstream's key_without_modifiers() behavior untouched; Android
uses the event's logical key as the best available layout label source.
"""
from pathlib import Path
import sys

root = Path(sys.argv[1])
path = root / "crates/horizon-client-input/src/keybinds.rs"
text = path.read_text()
marker = "// Horizon Android logical-key fallback"
if marker in text:
    print("Android keybind layout fallback already patched")
    raise SystemExit(0)

old = '''pub fn learn_layout(event: &winit::event::KeyEvent) {
    use winit::keyboard::{Key, PhysicalKey};
    use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
    let PhysicalKey::Code(code) = event.physical_key else {
        return;
    };
    let Some(name) = key_name(code) else {
        return;
    };
    let learn = horizon_client_core::key_display::learn_layout;
    match event.key_without_modifiers() {
        Key::Character(text) => learn(name, &text),
        Key::Dead(Some(c)) => learn(name, c.encode_utf8(&mut [0; 4])),
        _ => {}
    }
}
'''
new = '''pub fn learn_layout(event: &winit::event::KeyEvent) {
    use winit::keyboard::{Key, PhysicalKey};
    #[cfg(not(target_os = "android"))]
    use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
    let PhysicalKey::Code(code) = event.physical_key else {
        return;
    };
    let Some(name) = key_name(code) else {
        return;
    };
    let learn = horizon_client_core::key_display::learn_layout;

    #[cfg(not(target_os = "android"))]
    match event.key_without_modifiers() {
        Key::Character(text) => learn(name, &text),
        Key::Dead(Some(c)) => learn(name, c.encode_utf8(&mut [0; 4])),
        _ => {}
    }

    // Horizon Android logical-key fallback: winit does not expose
    // KeyEventExtModifierSupplement on Android, so retain layout learning from
    // the platform logical key without changing any desktop behavior.
    #[cfg(target_os = "android")]
    match &event.logical_key {
        Key::Character(text) => learn(name, text.as_str()),
        Key::Dead(Some(c)) => learn(name, c.encode_utf8(&mut [0; 4])),
        _ => {}
    }
}
'''
if old not in text:
    raise SystemExit("patch-android-keybinds: learn_layout anchor changed")
path.write_text(text.replace(old, new, 1))
print("patched Horizon Android keybind layout fallback")
