#!/usr/bin/env python3
"""Wire Android system haptics into the safe client and Bedrock touch UI."""

from pathlib import Path
import sys

root = Path(sys.argv[1])
android = root / "crates/horizon-client/src/android.rs"
android_shell = root / "crates/horizon-android/src/lib.rs"
touch = root / "crates/horizon-client/src/game/touch.rs"
settings_ui = root / "crates/horizon-client/src/game/touch_settings.rs"


def replace_once(path: Path, old: str, new: str, marker: str) -> None:
    text = path.read_text()
    if marker in text:
        return
    if old not in text:
        raise SystemExit(f"patch-touch-haptics: anchor not found in {path}: {old[:140]!r}")
    path.write_text(text.replace(old, new, 1))


# Safe callback boundary in horizon-client.
replace_once(
    android,
    'static RECREATE: Mutex<Option<fn()>> = Mutex::new(None);\n',
    '''static RECREATE: Mutex<Option<fn()>> = Mutex::new(None);
static HAPTIC: Mutex<Option<fn(Haptic)>> = Mutex::new(None);

#[derive(Clone, Copy, Debug)]
pub enum Haptic {
    Tap,
    Break,
}
''',
    'static HAPTIC: Mutex<Option<fn(Haptic)>>',
)
replace_once(
    android,
    '''/// Recreate NativeActivity on Android's UI thread. Its winit loop receives
''',
    '''/// The Android shell owns JNI and provides system haptic feedback through
/// this safe callback. Missing haptics are intentionally a no-op.
pub fn set_haptic_hook(hook: fn(Haptic)) {
    *HAPTIC.lock().unwrap_or_else(|e| e.into_inner()) = Some(hook);
}

pub fn haptic(kind: Haptic) {
    if let Some(hook) = *HAPTIC.lock().unwrap_or_else(|e| e.into_inner()) {
        hook(kind);
    }
}

/// Recreate NativeActivity on Android's UI thread. Its winit loop receives
''',
    'pub fn set_haptic_hook(hook: fn(Haptic))',
)

# Compile/install the JNI bridge from the shell, not the unsafe-forbidden client.
replace_once(
    android_shell,
    'mod instances;\n',
    'mod instances;\nmod haptics;\n',
    'mod haptics;',
)
replace_once(
    android_shell,
    '''    if !horizon_client::android::set_app(app.clone()) {
        say("Horizon: the activity was already handed over");
        return;
    }

    let played_from = std::time::Instant::now();
''',
    '''    if !horizon_client::android::set_app(app.clone()) {
        say("Horizon: the activity was already handed over");
        return;
    }
    haptics::install();

    let played_from = std::time::Instant::now();
''',
    'haptics::install();',
)

# Two independent player-facing haptic settings.
replace_once(
    touch,
    '''    hotbar_only: bool,
    left_handed: bool,
''',
    '''    hotbar_only: bool,
    button_haptics: bool,
    vibrate_breaking: bool,
    left_handed: bool,
''',
    'button_haptics: bool,',
)
replace_once(
    touch,
    '''            hotbar_only: false,
            left_handed: false,
''',
    '''            hotbar_only: false,
            button_haptics: true,
            vibrate_breaking: true,
            left_handed: false,
''',
    'button_haptics: true,',
)
replace_once(
    touch,
    '''                        "hotbar_only" | "touch_hotbar_only" => out.hotbar_only = yes,
                        "left_handed" | "left_handed_inventory" => out.left_handed = yes,
''',
    '''                        "hotbar_only" | "touch_hotbar_only" => out.hotbar_only = yes,
                        "button_haptics" => out.button_haptics = yes,
                        "vibrate_breaking" => out.vibrate_breaking = yes,
                        "left_handed" | "left_handed_inventory" => out.left_handed = yes,
''',
    '"button_haptics" => out.button_haptics = yes,',
)

# Haptic on a deliberate on-screen action press. Camera movement remains silent.
replace_once(
    touch,
    '''    fn press(&mut self, control: Control, down: bool) {
        let result = match control {
''',
    '''    fn press(&mut self, control: Control, down: bool) {
        if down && self.touch_ui.settings.button_haptics {
            crate::android::haptic(crate::android::Haptic::Tap);
        }
        let result = match control {
''',
    'crate::android::haptic(crate::android::Haptic::Tap)',
)

# Hotbar selection gets a light tick only when the slot actually changes.
replace_once(
    touch,
    '''                if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                    self.selected = slot;
                    Gesture::Hotbar
''',
    '''                if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                    if slot != self.selected && self.touch_ui.settings.button_haptics {
                        crate::android::haptic(crate::android::Haptic::Tap);
                    }
                    self.selected = slot;
                    Gesture::Hotbar
''',
    'slot != self.selected && self.touch_ui.settings.button_haptics',
)
replace_once(
    touch,
    '''                        if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                            self.selected = slot;
                        }
''',
    '''                        if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                            if slot != self.selected && self.touch_ui.settings.button_haptics {
                                crate::android::haptic(crate::android::Haptic::Tap);
                            }
                            self.selected = slot;
                        }
''',
    'Gesture::Hotbar => {\n                        if let Some(slot)',
)
# The marker above exists already as context; if replacement ran, add a durable marker.
text = touch.read_text()
if 'Horizon Android hotbar haptic' not in text:
    text = text.replace('Gesture::Hotbar => {\n                        if let Some(slot)', '// Horizon Android hotbar haptic\n                    Gesture::Hotbar => {\n                        if let Some(slot)', 1)
touch.write_text(text)

# Long-press mining feedback when breaking starts.
replace_once(
    touch,
    '''        if self.play_mouse(MouseButton::Left, true).is_ok()
            && let Some(Gesture::Look { attacking, .. }) = self.touch_ui.fingers.get_mut(&id)
        {
            *attacking = true;
        }
''',
    '''        if self.play_mouse(MouseButton::Left, true).is_ok()
            && let Some(Gesture::Look { attacking, .. }) = self.touch_ui.fingers.get_mut(&id)
        {
            *attacking = true;
            if self.touch_ui.settings.vibrate_breaking {
                crate::android::haptic(crate::android::Haptic::Break);
            }
        }
''',
    'crate::android::haptic(crate::android::Haptic::Break)',
)

# Persist and expose the two settings through the in-game panel.
replace_once(
    touch,
    '''    pub(super) fn touch_handedness_name(&self) -> &'static str {
''',
    '''    pub(super) fn touch_button_haptics_name(&self) -> &'static str {
        if self.touch_ui.settings.button_haptics { "On" } else { "Off" }
    }

    pub(super) fn touch_break_vibration_name(&self) -> &'static str {
        if self.touch_ui.settings.vibrate_breaking { "On" } else { "Off" }
    }

    pub(super) fn touch_handedness_name(&self) -> &'static str {
''',
    'pub(super) fn touch_button_haptics_name(&self)',
)
replace_once(
    touch,
    '''\\nhotbar_only={}\\nleft_handed={}\\njoystick_visibility={visibility}\\n",
''',
    '''\\nhotbar_only={}\\nbutton_haptics={}\\nvibrate_breaking={}\\nleft_handed={}\\njoystick_visibility={visibility}\\n",
''',
    'button_haptics={}\\nvibrate_breaking={}',
)
replace_once(
    touch,
    '''            settings.hotbar_only,
            settings.left_handed,
''',
    '''            settings.hotbar_only,
            settings.button_haptics,
            settings.vibrate_breaking,
            settings.left_handed,
''',
    'settings.button_haptics,\n            settings.vibrate_breaking,',
)
replace_once(
    touch,
    '''    pub(super) fn touch_toggle_handedness(&mut self) -> Result<String, String> {
''',
    '''    pub(super) fn touch_toggle_button_haptics(&mut self) -> Result<String, String> {
        self.touch_ui.settings.button_haptics = !self.touch_ui.settings.button_haptics;
        self.persist_touch_settings()
    }

    pub(super) fn touch_toggle_break_vibration(&mut self) -> Result<String, String> {
        self.touch_ui.settings.vibrate_breaking = !self.touch_ui.settings.vibrate_breaking;
        self.persist_touch_settings()
    }

    pub(super) fn touch_toggle_handedness(&mut self) -> Result<String, String> {
''',
    'pub(super) fn touch_toggle_button_haptics(&mut self)',
)

# Compact the now-long touch settings list so it remains usable on landscape
# phones at Horizon's GUI scale.
replace_once(settings_ui, 'const ROW_H: f32 = 27.0;\n', 'const ROW_H: f32 = 22.0;\n', 'const ROW_H: f32 = 22.0;')
replace_once(
    settings_ui,
    '''            ("Touch hotbar only", self.touch_hotbar_only_name()),
        ];
''',
    '''            ("Touch hotbar only", self.touch_hotbar_only_name()),
            ("Button haptics", self.touch_button_haptics_name()),
            ("Break vibration", self.touch_break_vibration_name()),
        ];
''',
    '"Break vibration", self.touch_break_vibration_name()',
)
replace_once(settings_ui, '        for i in 0..9 {\n', '        for i in 0..11 {\n', 'for i in 0..11 {')
replace_once(
    settings_ui,
    '''                    8 => self.touch_toggle_hotbar_only(),
                    _ => unreachable!(),
''',
    '''                    8 => self.touch_toggle_hotbar_only(),
                    9 => self.touch_toggle_button_haptics(),
                    10 => self.touch_toggle_break_vibration(),
                    _ => unreachable!(),
''',
    '10 => self.touch_toggle_break_vibration()',
)

print("patched Horizon Android touch haptics")
