#!/usr/bin/env python3
"""Wire the Android touch settings panel into Horizon's game worktree.

The large upstream game loop and pause screen remain source-parity friendly;
Android-only hooks are applied while packaging, like the existing lifecycle and
launcherless manager hooks.
"""

from pathlib import Path
import sys

root = Path(sys.argv[1])
game = root / "crates/horizon-client/src/game.rs"
touch = root / "crates/horizon-client/src/game/touch.rs"
mobile = root / "crates/horizon-client/src/game/mobile_ui.rs"


def replace_once(path: Path, old: str, new: str, marker: str) -> None:
    text = path.read_text()
    if marker in text:
        return
    if old not in text:
        raise SystemExit(f"patch-touch-settings: anchor not found in {path}: {old[:120]!r}")
    path.write_text(text.replace(old, new, 1))


# Compile the panel beside the Android touch and launcherless UI modules.
replace_once(
    game,
    '#[cfg(target_os = "android")]\nmod mobile_ui; // Horizon Android launcherless manager\n',
    '#[cfg(target_os = "android")]\nmod mobile_ui; // Horizon Android launcherless manager\n'
    '#[cfg(target_os = "android")]\nmod touch_settings; // Horizon Android Bedrock touch settings\n',
    'mod touch_settings; // Horizon Android Bedrock touch settings',
)

# The settings panel deliberately talks to Game methods instead of reaching into
# Touch's private state. These helpers apply immediately, save per instance, and
# keep the Android crosshair presentation in sync without restarting Horizon.
replace_once(
    touch,
    '''    pub(super) fn set_soft_keyboard(&self, show: bool) {
''',
    '''    pub(super) fn touch_control_mode_name(&self) -> &'static str {
        match self.touch_ui.settings.preset {
            Preset::JoystickTap => "Joystick & tap",
            Preset::JoystickCrosshair => "Joystick & crosshair",
            Preset::DpadTap => "D-pad & tap",
        }
    }

    pub(super) fn touch_joystick_mode_name(&self) -> &'static str {
        if self.touch_ui.settings.static_joystick { "Static" } else { "Floating" }
    }

    pub(super) fn touch_action_buttons_name(&self) -> &'static str {
        if self.touch_ui.settings.show_action_buttons { "On" } else { "Off" }
    }

    pub(super) fn touch_handedness_name(&self) -> &'static str {
        if self.touch_ui.settings.left_handed { "Left handed" } else { "Right handed" }
    }

    pub(super) fn touch_joystick_visibility_name(&self) -> &'static str {
        match self.touch_ui.settings.joystick_visibility {
            JoystickVisibility::Always => "Always",
            JoystickVisibility::WhenTouched => "When touched",
            JoystickVisibility::Hidden => "Hidden",
        }
    }

    fn persist_touch_settings(&self) -> Result<String, String> {
        let settings = self.touch_ui.settings;
        let mode = match settings.preset {
            Preset::JoystickTap => "joystick",
            Preset::JoystickCrosshair => "crosshair",
            Preset::DpadTap => "dpad",
        };
        let visibility = match settings.joystick_visibility {
            JoystickVisibility::Always => "always",
            JoystickVisibility::WhenTouched => "touch",
            JoystickVisibility::Hidden => "hidden",
        };
        let body = format!(
            "mode={mode}\\nstatic_joystick={}\\nshow_action_buttons={}\\nleft_handed={}\\njoystick_visibility={visibility}\\n",
            settings.static_joystick,
            settings.show_action_buttons,
            settings.left_handed,
        );
        std::fs::write("horizon-touch.txt", body)
            .map_err(|error| format!("Could not save touch settings: {error}"))?;
        crate::hud_layout::set_android_touch_crosshair(matches!(settings.preset, Preset::JoystickCrosshair));
        Ok("Touch settings saved".into())
    }

    pub(super) fn touch_cycle_control_mode(&mut self) -> Result<String, String> {
        self.release_touch_inputs();
        self.touch_ui.settings.preset = match self.touch_ui.settings.preset {
            Preset::JoystickTap => Preset::JoystickCrosshair,
            Preset::JoystickCrosshair => Preset::DpadTap,
            Preset::DpadTap => Preset::JoystickTap,
        };
        self.persist_touch_settings()
    }

    pub(super) fn touch_toggle_static_joystick(&mut self) -> Result<String, String> {
        self.release_touch_inputs();
        self.touch_ui.settings.static_joystick = !self.touch_ui.settings.static_joystick;
        self.persist_touch_settings()
    }

    pub(super) fn touch_toggle_action_buttons(&mut self) -> Result<String, String> {
        self.release_touch_inputs();
        self.touch_ui.settings.show_action_buttons = !self.touch_ui.settings.show_action_buttons;
        self.persist_touch_settings()
    }

    pub(super) fn touch_toggle_handedness(&mut self) -> Result<String, String> {
        self.release_touch_inputs();
        self.touch_ui.settings.left_handed = !self.touch_ui.settings.left_handed;
        self.persist_touch_settings()
    }

    pub(super) fn touch_cycle_joystick_visibility(&mut self) -> Result<String, String> {
        self.touch_ui.settings.joystick_visibility = match self.touch_ui.settings.joystick_visibility {
            JoystickVisibility::Always => JoystickVisibility::WhenTouched,
            JoystickVisibility::WhenTouched => JoystickVisibility::Hidden,
            JoystickVisibility::Hidden => JoystickVisibility::Always,
        };
        self.persist_touch_settings()
    }

    pub(super) fn touch_reset_settings(&mut self) -> Result<String, String> {
        self.release_touch_inputs();
        self.touch_ui.settings = Settings::default();
        self.persist_touch_settings()?;
        Ok("Touch controls reset to Bedrock-style defaults".into())
    }

    pub(super) fn set_soft_keyboard(&self, show: bool) {
''',
    'pub(super) fn touch_control_mode_name(&self)',
)

# Put the entry chip above the instance chip. The panel itself is modal but the
# normal launcherless instance manager remains available when it is closed.
replace_once(
    mobile,
    '''    pub(super) fn draw_android_pause_controls(&mut self, batch: &mut HudBatch) {
        let Some(root) = root() else { return };
''',
    '''    pub(super) fn draw_android_pause_controls(&mut self, batch: &mut HudBatch) {
        self.draw_android_touch_settings_entry(batch);
        let Some(root) = root() else { return };
''',
    'self.draw_android_touch_settings_entry(batch)',
)

# Touch settings get first refusal while open so a tap cannot also activate a
# vanilla pause button or the instance panel underneath.
replace_once(
    mobile,
    '''    pub(super) fn android_pause_click(&mut self) -> bool {
        let Some(root) = root() else { return false };
''',
    '''    pub(super) fn android_pause_click(&mut self) -> bool {
        if self.android_touch_settings_click() {
            return true;
        }
        let Some(root) = root() else { return false };
''',
    'if self.android_touch_settings_click()',
)

print("patched Horizon Android in-game touch settings")
