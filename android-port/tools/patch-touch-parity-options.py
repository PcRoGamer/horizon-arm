#!/usr/bin/env python3
"""Fill out Bedrock mobile touch-mode behavior beyond the three main presets."""

from pathlib import Path
import sys

root = Path(sys.argv[1])
touch = root / "crates/horizon-client/src/game/touch.rs"
settings_ui = root / "crates/horizon-client/src/game/touch_settings.rs"


def replace_once(path: Path, old: str, new: str, marker: str) -> None:
    text = path.read_text()
    if marker in text:
        return
    if old not in text:
        raise SystemExit(f"patch-touch-parity-options: anchor not found in {path}: {old[:140]!r}")
    path.write_text(text.replace(old, new, 1))


# Persisted core toggles.
replace_once(
    touch,
    '''    show_action_buttons: bool,
    left_handed: bool,
    joystick_visibility: JoystickVisibility,
''',
    '''    show_action_buttons: bool,
    split_controls: bool,
    swap_jump_sneak: bool,
    invert_y: bool,
    hotbar_only: bool,
    left_handed: bool,
    joystick_visibility: JoystickVisibility,
''',
    'split_controls: bool,',
)
replace_once(
    touch,
    '''            show_action_buttons: true,
            left_handed: false,
            joystick_visibility: JoystickVisibility::Always,
''',
    '''            show_action_buttons: true,
            split_controls: false,
            swap_jump_sneak: false,
            invert_y: false,
            hotbar_only: false,
            left_handed: false,
            joystick_visibility: JoystickVisibility::Always,
''',
    'split_controls: false,',
)
replace_once(
    touch,
    '''                        "show_action_buttons" => out.show_action_buttons = yes,
                        "left_handed" | "left_handed_inventory" => out.left_handed = yes,
''',
    '''                        "show_action_buttons" => out.show_action_buttons = yes,
                        "split_controls" => out.split_controls = yes,
                        "swap_jump_sneak" => out.swap_jump_sneak = yes,
                        "invert_y" => out.invert_y = yes,
                        "hotbar_only" | "touch_hotbar_only" => out.hotbar_only = yes,
                        "left_handed" | "left_handed_inventory" => out.left_handed = yes,
''',
    '"split_controls" => out.split_controls = yes,',
)
replace_once(
    touch,
    '''    fn tap_interaction(self) -> bool {
        self.preset != Preset::JoystickCrosshair || !self.show_action_buttons
    }
''',
    '''    fn crosshair_mode(self) -> bool {
        self.preset == Preset::JoystickCrosshair
            || (self.preset == Preset::DpadTap && self.split_controls)
    }

    fn tap_interaction(self) -> bool {
        !self.crosshair_mode() || !self.show_action_buttons
    }
''',
    'fn crosshair_mode(self) -> bool',
)

# Crosshair action buttons are also used by D-pad Split Controls.
replace_once(
    touch,
    '        if settings.preset == Preset::JoystickCrosshair && settings.show_action_buttons {\n',
    '        if settings.crosshair_mode() && settings.show_action_buttons {\n',
    'if settings.crosshair_mode() && settings.show_action_buttons',
)

# Mirror Bedrock's jump/sneak swap and controller-friendly hotbar-only mode.
replace_once(
    touch,
    '''        let movement_x = if settings.left_handed {
''',
    '''        if settings.swap_jump_sneak {
            let jump = buttons.iter().position(|(c, _, _)| *c == Jump);
            let sneak = buttons.iter().position(|(c, _, _)| *c == Sneak);
            if let (Some(j), Some(s)) = (jump, sneak) {
                let jump_centre = buttons[j].1;
                buttons[j].1 = buttons[s].1;
                buttons[s].1 = jump_centre;
            }
        }
        if settings.hotbar_only {
            buttons.retain(|(control, _, _)| matches!(control, Pause | Chat | Perspective | Inventory));
        }

        let movement_x = if settings.left_handed {
''',
    'if settings.swap_jump_sneak {',
)

# In hotbar-only mode, empty world touches do nothing instead of moving camera.
replace_once(
    touch,
    '''                } else if let Some(vector) = layout.dpad_vector(pos) {
''',
    '''                } else if settings.hotbar_only {
                    return;
                } else if let Some(vector) = layout.dpad_vector(pos) {
''',
    '} else if settings.hotbar_only {',
)

# Camera Y inversion is applied only to look movement; control drag uses its own
# editor path and therefore never gets inverted.
replace_once(
    touch,
    '''                        *travel += (dx * dx + dy * dy).sqrt();
                        *last = pos;
                        self.mouse.on_move(0.0, 0.0, dx, dy, true, true);
''',
    '''                        *travel += (dx * dx + dy * dy).sqrt();
                        *last = pos;
                        let look_dy = if settings.invert_y { -dy } else { dy };
                        self.mouse.on_move(0.0, 0.0, dx, look_dy, true, true);
''',
    'let look_dy = if settings.invert_y',
)

# Hotbar-only hides movement controls entirely. Use unique begin/end markers;
# the original source already contains the brace sequence around this match, so
# using that brace sequence as an idempotence marker falsely skipped the close.
replace_once(
    touch,
    '''        match settings.preset {
            Preset::DpadTap => touch_icons::dpad(
''',
    '''        // Horizon Android hotbar-only movement HUD begin
        if !settings.hotbar_only {
        match settings.preset {
            Preset::DpadTap => touch_icons::dpad(
''',
    '// Horizon Android hotbar-only movement HUD begin',
)
replace_once(
    touch,
    '''            }
        }

        for (control, (cx, cy), r) in &layout.buttons {
''',
    '''            }
        }
        } // Horizon Android hotbar-only movement HUD end

        for (control, (cx, cy), r) in &layout.buttons {
''',
    '// Horizon Android hotbar-only movement HUD end',
)

# Expand the methods injected by patch-touch-settings.py and persist all toggles.
replace_once(
    touch,
    '''    pub(super) fn touch_handedness_name(&self) -> &'static str {
''',
    '''    pub(super) fn touch_split_controls_name(&self) -> &'static str {
        if self.touch_ui.settings.split_controls { "On" } else { "Off" }
    }

    pub(super) fn touch_swap_jump_sneak_name(&self) -> &'static str {
        if self.touch_ui.settings.swap_jump_sneak { "On" } else { "Off" }
    }

    pub(super) fn touch_invert_y_name(&self) -> &'static str {
        if self.touch_ui.settings.invert_y { "On" } else { "Off" }
    }

    pub(super) fn touch_hotbar_only_name(&self) -> &'static str {
        if self.touch_ui.settings.hotbar_only { "On" } else { "Off" }
    }

    pub(super) fn touch_handedness_name(&self) -> &'static str {
''',
    'pub(super) fn touch_split_controls_name(&self)',
)
replace_once(
    touch,
    '''            "mode={mode}\\nstatic_joystick={}\\nshow_action_buttons={}\\nleft_handed={}\\njoystick_visibility={visibility}\\n",
            settings.static_joystick,
            settings.show_action_buttons,
            settings.left_handed,
''',
    '''            "mode={mode}\\nstatic_joystick={}\\nshow_action_buttons={}\\nsplit_controls={}\\nswap_jump_sneak={}\\ninvert_y={}\\nhotbar_only={}\\nleft_handed={}\\njoystick_visibility={visibility}\\n",
            settings.static_joystick,
            settings.show_action_buttons,
            settings.split_controls,
            settings.swap_jump_sneak,
            settings.invert_y,
            settings.hotbar_only,
            settings.left_handed,
''',
    'split_controls={}\\nswap_jump_sneak={}',
)
replace_once(
    touch,
    'crate::hud_layout::set_android_touch_crosshair(matches!(settings.preset, Preset::JoystickCrosshair));',
    'crate::hud_layout::set_android_touch_crosshair(settings.crosshair_mode());',
    'set_android_touch_crosshair(settings.crosshair_mode())',
)
replace_once(
    touch,
    '''    pub(super) fn touch_toggle_handedness(&mut self) -> Result<String, String> {
''',
    '''    pub(super) fn touch_toggle_split_controls(&mut self) -> Result<String, String> {
        self.release_touch_inputs();
        self.touch_ui.settings.split_controls = !self.touch_ui.settings.split_controls;
        self.persist_touch_settings()
    }

    pub(super) fn touch_toggle_swap_jump_sneak(&mut self) -> Result<String, String> {
        self.release_touch_inputs();
        self.touch_ui.settings.swap_jump_sneak = !self.touch_ui.settings.swap_jump_sneak;
        self.persist_touch_settings()
    }

    pub(super) fn touch_toggle_invert_y(&mut self) -> Result<String, String> {
        self.touch_ui.settings.invert_y = !self.touch_ui.settings.invert_y;
        self.persist_touch_settings()
    }

    pub(super) fn touch_toggle_hotbar_only(&mut self) -> Result<String, String> {
        self.release_touch_inputs();
        self.touch_ui.settings.hotbar_only = !self.touch_ui.settings.hotbar_only;
        self.persist_touch_settings()
    }

    pub(super) fn touch_toggle_handedness(&mut self) -> Result<String, String> {
''',
    'pub(super) fn touch_toggle_split_controls(&mut self)',
)

# Settings panel: enough height for the full Bedrock-oriented list.
replace_once(settings_ui, 'const PANEL_H: f32 = 246.0;\n', 'const PANEL_H: f32 = 346.0;\n', 'const PANEL_H: f32 = 346.0;')
replace_once(
    settings_ui,
    '''        let rows = [
            ("Control mode", self.touch_control_mode_name()),
            ("Joystick", self.touch_joystick_mode_name()),
            ("Action buttons", self.touch_action_buttons_name()),
            ("Handedness", self.touch_handedness_name()),
            ("Joystick visibility", self.touch_joystick_visibility_name()),
        ];
''',
    '''        let rows = [
            ("Control mode", self.touch_control_mode_name()),
            ("Joystick", self.touch_joystick_mode_name()),
            ("Action buttons", self.touch_action_buttons_name()),
            ("Split controls", self.touch_split_controls_name()),
            ("Swap jump / sneak", self.touch_swap_jump_sneak_name()),
            ("Handedness", self.touch_handedness_name()),
            ("Joystick visibility", self.touch_joystick_visibility_name()),
            ("Invert camera Y", self.touch_invert_y_name()),
            ("Touch hotbar only", self.touch_hotbar_only_name()),
        ];
''',
    '"Touch hotbar only", self.touch_hotbar_only_name()',
)
replace_once(settings_ui, '        for i in 0..5 {\n', '        for i in 0..9 {\n', 'for i in 0..9 {')
replace_once(
    settings_ui,
    '''                    3 => self.touch_toggle_handedness(),
                    4 => self.touch_cycle_joystick_visibility(),
''',
    '''                    3 => self.touch_toggle_split_controls(),
                    4 => self.touch_toggle_swap_jump_sneak(),
                    5 => self.touch_toggle_handedness(),
                    6 => self.touch_cycle_joystick_visibility(),
                    7 => self.touch_toggle_invert_y(),
                    8 => self.touch_toggle_hotbar_only(),
''',
    '8 => self.touch_toggle_hotbar_only()',
)

print("patched Horizon Android Bedrock touch parity options")
