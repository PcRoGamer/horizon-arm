#!/usr/bin/env python3
"""Keep Horizon's clipboard/text editing on Android without compiling arboard.

arboard 3.6 has no Android backend. Desktop remains byte-for-byte upstream:
only the disposable Android build tree gets this target gate. Android retains
Horizon's in-process copied-text fallback so copy/cut/paste keep working inside
the game while the native system bridge can be supplied separately.
"""
from pathlib import Path
import sys

root = Path(sys.argv[1])
manifest = root / "crates/horizon-client-input/Cargo.toml"
clipboard = root / "crates/horizon-client-input/src/clipboard.rs"

m = manifest.read_text()
dep = 'arboard = { version = "3.6.1", default-features = false, features = ["wayland-data-control"] }\n'
if "[target.'cfg(not(target_os = \"android\"))'.dependencies]" not in m:
    if dep not in m:
        raise SystemExit("patch-android-clipboard: arboard dependency anchor changed")
    m = m.replace(dep, "", 1)
    anchor = "\n[features]\n"
    if anchor not in m:
        raise SystemExit("patch-android-clipboard: features anchor changed")
    m = m.replace(
        anchor,
        "\n[target.'cfg(not(target_os = \"android\"))'.dependencies]\n" + dep + anchor,
        1,
    )
manifest.write_text(m)

text = clipboard.read_text()

old = '''#[derive(Default)]
pub struct SystemClipboard {
    system: Option<arboard::Clipboard>,
    /// The text Horizon last copied (`horizon-ctl status`).
    copied: Option<String>,
}'''
new = '''#[derive(Default)]
pub struct SystemClipboard {
    #[cfg(not(target_os = "android"))]
    system: Option<arboard::Clipboard>,
    /// The text Horizon last copied (`horizon-ctl status`). On Android this is
    /// also the process-local fallback when no system clipboard backend exists.
    copied: Option<String>,
}'''
if old in text:
    text = text.replace(old, new, 1)
elif new not in text:
    raise SystemExit("patch-android-clipboard: SystemClipboard struct anchor changed")

old = '''    fn system(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.system.is_none() {
            self.system = arboard::Clipboard::new().ok();
        }
        self.system.as_mut()
    }'''
new = '''    #[cfg(not(target_os = "android"))]
    fn system(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.system.is_none() {
            self.system = arboard::Clipboard::new().ok();
        }
        self.system.as_mut()
    }'''
if old in text:
    text = text.replace(old, new, 1)
elif new not in text:
    raise SystemExit("patch-android-clipboard: SystemClipboard::system anchor changed")

old = '''    fn get(&mut self) -> String {
        let Some(system) = self.system() else {
            return self.copied.clone().unwrap_or_default();
        };
        match system.get_text() {
            Ok(text) => text,
            Err(arboard::Error::ContentNotAvailable) => String::new(),
            Err(_) => {
                // A lost connection (a restarted compositor): reconnect next time.
                self.system = None;
                String::new()
            }
        }
    }'''
new = '''    fn get(&mut self) -> String {
        #[cfg(target_os = "android")]
        {
            self.copied.clone().unwrap_or_default()
        }
        #[cfg(not(target_os = "android"))]
        {
            let Some(system) = self.system() else {
                return self.copied.clone().unwrap_or_default();
            };
            match system.get_text() {
                Ok(text) => text,
                Err(arboard::Error::ContentNotAvailable) => String::new(),
                Err(_) => {
                    // A lost connection (a restarted compositor): reconnect next time.
                    self.system = None;
                    String::new()
                }
            }
        }
    }'''
if old in text:
    text = text.replace(old, new, 1)
elif new not in text:
    raise SystemExit("patch-android-clipboard: Clipboard::get anchor changed")

old = '''    fn set(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.copied = Some(text.to_owned());
        if let Some(system) = self.system()
            && system.set_text(text.to_owned()).is_err()
        {
            self.system = None;
        }
    }'''
new = '''    fn set(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.copied = Some(text.to_owned());
        #[cfg(not(target_os = "android"))]
        if let Some(system) = self.system()
            && system.set_text(text.to_owned()).is_err()
        {
            self.system = None;
        }
    }'''
if old in text:
    text = text.replace(old, new, 1)
elif new not in text:
    raise SystemExit("patch-android-clipboard: Clipboard::set anchor changed")

clipboard.write_text(text)
print("patched Android clipboard: arboard desktop-only, in-game fallback retained")
