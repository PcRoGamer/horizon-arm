#!/usr/bin/env python3
"""Add Bedrock-style drag/resize/opacity touch customization to Android builds.

The editor is applied in the Android build worktree so desktop Horizon remains
upstream-shaped. Layout data is stored per instance in horizon-touch-layout.txt.
"""

from pathlib import Path
import sys

root = Path(sys.argv[1])
touch = root / "crates/horizon-client/src/game/touch.rs"
icons = root / "crates/horizon-client/src/game/touch_icons.rs"
settings_ui = root / "crates/horizon-client/src/game/touch_settings.rs"


def replace_once(path: Path, old: str, new: str, marker: str) -> None:
    text = path.read_text()
    if marker in text:
        return
    if old not in text:
        raise SystemExit(f"patch-touch-editor: anchor not found in {path}: {old[:140]!r}")
    path.write_text(text.replace(old, new, 1))


# ----- icon opacity -------------------------------------------------------
replace_once(
    icons,
    'use horizon_client::hud::{HudAtlas, HudBatch, rgba};\n',
    '''use horizon_client::hud::{HudAtlas, HudBatch, rgba};

fn alpha(base: u8, opacity: f32) -> u8 {
    (f32::from(base) * opacity.clamp(0.12, 1.0)).round().clamp(0.0, 255.0) as u8
}
''',
    'fn alpha(base: u8, opacity: f32)',
)
replace_once(
    icons,
    '''fn ring_plate(batch: &mut HudBatch, atlas: &HudAtlas, cx: f32, cy: f32, r: f32, down: bool) {
    plate(
        batch,
        atlas,
        cx,
        cy,
        r,
        rgba(255, 255, 255, if down { 122 } else { 58 }),
    );
    plate(
        batch,
        atlas,
        cx,
        cy,
        (r - 1.4).max(1.0),
        rgba(18, 20, 23, if down { 148 } else { 94 }),
    );
}
''',
    '''fn ring_plate(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    cx: f32,
    cy: f32,
    r: f32,
    down: bool,
    opacity: f32,
) {
    plate(
        batch,
        atlas,
        cx,
        cy,
        r,
        rgba(255, 255, 255, alpha(if down { 122 } else { 58 }, opacity)),
    );
    plate(
        batch,
        atlas,
        cx,
        cy,
        (r - 1.4).max(1.0),
        rgba(18, 20, 23, alpha(if down { 148 } else { 94 }, opacity)),
    );
}
''',
    'opacity: f32,\n) {\n    plate(\n        batch,\n        atlas,\n        cx,\n        cy,\n        r,',
)
replace_once(
    icons,
    '''    vector: [f32; 2],
) {
''',
    '''    vector: [f32; 2],
    opacity: f32,
) {
''',
    'vector: [f32; 2],\n    opacity: f32,',
)
for old, new in [
    ('rgba(255, 255, 255, 24)', 'rgba(255, 255, 255, alpha(24, opacity))'),
    ('rgba(17, 19, 22, 50)', 'rgba(17, 19, 22, alpha(50, opacity))'),
    ('rgba(255, 255, 255, if moving { 122 } else { 64 })', 'rgba(255, 255, 255, alpha(if moving { 122 } else { 64 }, opacity))'),
    ('rgba(38, 40, 44, if moving { 104 } else { 72 })', 'rgba(38, 40, 44, alpha(if moving { 104 } else { 72 }, opacity))'),
]:
    replace_once(icons, old, new, new)

# D-pad: add opacity and pass it through the plates/arrows.
replace_once(
    icons,
    '''    r: f32,
    vector: [f32; 2],
) {
    let arm = r * 0.46;
''',
    '''    r: f32,
    vector: [f32; 2],
    opacity: f32,
) {
    let arm = r * 0.46;
''',
    'let arm = r * 0.46; // editor opacity',
)
# The previous replacement marker is deliberately inserted into the body.
text = icons.read_text()
if 'let arm = r * 0.46; // editor opacity' not in text:
    text = text.replace('    let arm = r * 0.46;\n', '    let arm = r * 0.46; // editor opacity\n', 1)
icons.write_text(text)
replace_once(icons, 'ring_plate(batch, atlas, x, y, arm, down);', 'ring_plate(batch, atlas, x, y, arm, down, opacity);', 'ring_plate(batch, atlas, x, y, arm, down, opacity);')
replace_once(icons, 'let color = rgba(255, 255, 255, if down { 255 } else { 210 });', 'let color = rgba(255, 255, 255, alpha(if down { 255 } else { 210 }, opacity));', 'alpha(if down { 255 } else { 210 }, opacity)')
replace_once(icons, 'plate(batch, atlas, cx, cy, gap, rgba(255, 255, 255, 32));', 'plate(batch, atlas, cx, cy, gap, rgba(255, 255, 255, alpha(32, opacity)));', 'alpha(32, opacity)')

# Button opacity.
replace_once(
    icons,
    '''    r: f32,
    down: bool,
) {
    ring_plate(batch, atlas, cx, cy, r, down);
''',
    '''    r: f32,
    down: bool,
    opacity: f32,
) {
    ring_plate(batch, atlas, cx, cy, r, down, opacity);
''',
    'ring_plate(batch, atlas, cx, cy, r, down, opacity);',
)
replace_once(
    icons,
    '        rgba(255, 255, 255, if down { 255 } else { 224 }),\n',
    '        rgba(255, 255, 255, alpha(if down { 255 } else { 224 }, opacity)),\n',
    'alpha(if down { 255 } else { 224 }, opacity)',
)

# ----- custom layout data ------------------------------------------------
replace_once(
    touch,
    'use horizon_client::hud::HudBatch;\n',
    'use horizon_client::hud::{HudBatch, rgba};\n',
    'use horizon_client::hud::{HudBatch, rgba};',
)
replace_once(
    touch,
    '''    fn momentary(self) -> bool {
        matches!(self, Self::Inventory | Self::Chat | Self::Pause | Self::Perspective)
    }
}

struct Layout {
''',
    '''    fn momentary(self) -> bool {
        matches!(self, Self::Inventory | Self::Chat | Self::Pause | Self::Perspective)
    }

    fn index(self) -> usize {
        match self {
            Self::Jump => 0,
            Self::Sneak => 1,
            Self::Sprint => 2,
            Self::Attack => 3,
            Self::Use => 4,
            Self::Inventory => 5,
            Self::Chat => 6,
            Self::Pause => 7,
            Self::Perspective => 8,
        }
    }

    fn from_layout_key(key: &str) -> Option<Self> {
        Some(match key {
            "jump" => Self::Jump,
            "sneak" => Self::Sneak,
            "sprint" => Self::Sprint,
            "attack" => Self::Attack,
            "use" => Self::Use,
            "inventory" => Self::Inventory,
            "chat" => Self::Chat,
            "pause" => Self::Pause,
            "perspective" => Self::Perspective,
            _ => return None,
        })
    }
}

const ALL_CONTROLS: [Control; 9] = [
    Control::Jump,
    Control::Sneak,
    Control::Sprint,
    Control::Attack,
    Control::Use,
    Control::Inventory,
    Control::Chat,
    Control::Pause,
    Control::Perspective,
];

#[derive(Clone, Copy, Debug)]
struct Placement {
    x: f32,
    y: f32,
    scale: f32,
    opacity: f32,
}

impl Placement {
    fn sane(mut self) -> Self {
        self.x = self.x.clamp(0.03, 0.97);
        self.y = self.y.clamp(0.04, 0.96);
        self.scale = self.scale.clamp(0.55, 1.80);
        self.opacity = self.opacity.clamp(0.20, 1.0);
        self
    }
}

#[derive(Clone, Copy, Debug)]
struct CustomLayout {
    movement: Option<Placement>,
    controls: [Option<Placement>; 9],
}

impl Default for CustomLayout {
    fn default() -> Self {
        Self { movement: None, controls: [None; 9] }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorTarget {
    Movement,
    Control(Control),
}

impl EditorTarget {
    fn label(self) -> &'static str {
        match self {
            Self::Movement => "Movement",
            Self::Control(control) => control.label(),
        }
    }
}

impl CustomLayout {
    fn load() -> Self {
        let Ok(text) = std::fs::read_to_string("horizon-touch-layout.txt") else {
            return Self::default();
        };
        let mut out = Self::default();
        for raw in text.lines() {
            let Some((key, value)) = raw.split_once('=') else { continue };
            let mut values = value.split(',').filter_map(|v| v.trim().parse::<f32>().ok());
            let (Some(x), Some(y), Some(scale), Some(opacity)) =
                (values.next(), values.next(), values.next(), values.next())
            else { continue };
            let placement = Placement { x, y, scale, opacity }.sane();
            let key = key.trim().strip_prefix("control.").unwrap_or(key.trim());
            if key == "movement" {
                out.movement = Some(placement);
            } else if let Some(control) = Control::from_layout_key(key) {
                out.controls[control.index()] = Some(placement);
            }
        }
        out
    }

    fn save(self) -> Result<(), String> {
        let mut out = String::new();
        if let Some(p) = self.movement {
            out.push_str(&format!("movement={:.5},{:.5},{:.3},{:.3}\\n", p.x, p.y, p.scale, p.opacity));
        }
        for control in ALL_CONTROLS {
            if let Some(p) = self.controls[control.index()] {
                out.push_str(&format!("control.{}={:.5},{:.5},{:.3},{:.3}\\n", control.label(), p.x, p.y, p.scale, p.opacity));
            }
        }
        std::fs::write("horizon-touch-layout.txt", out)
            .map_err(|error| format!("Could not save control layout: {error}"))
    }

    fn placement(self, target: EditorTarget) -> Option<Placement> {
        match target {
            EditorTarget::Movement => self.movement,
            EditorTarget::Control(control) => self.controls[control.index()],
        }
    }

    fn set(&mut self, target: EditorTarget, placement: Placement) {
        match target {
            EditorTarget::Movement => self.movement = Some(placement.sane()),
            EditorTarget::Control(control) => self.controls[control.index()] = Some(placement.sane()),
        }
    }

    fn opacity(self, target: EditorTarget) -> f32 {
        self.placement(target).map_or(1.0, |p| p.opacity)
    }

    fn apply(self, layout: &mut Layout, w: f32, h: f32) {
        if let Some(p) = self.movement {
            layout.default_stick = (p.x * w, p.y * h);
            layout.dpad_center = layout.default_stick;
            layout.stick_radius *= p.scale;
            layout.dpad_radius *= p.scale;
        }
        for (control, centre, radius) in &mut layout.buttons {
            if let Some(p) = self.controls[control.index()] {
                *centre = (p.x * w, p.y * h);
                *radius *= p.scale;
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TouchEditor {
    active: bool,
    selected: Option<EditorTarget>,
    finger: Option<u64>,
    drag_offset: (f64, f64),
}

impl Default for TouchEditor {
    fn default() -> Self {
        Self { active: false, selected: None, finger: None, drag_offset: (0.0, 0.0) }
    }
}

struct Layout {
''',
    'struct TouchEditor {',
)

replace_once(
    touch,
    '    fn new((w, h): (f32, f32), settings: Settings) -> Self {\n',
    '    fn new((w, h): (f32, f32), settings: Settings, custom: CustomLayout) -> Self {\n',
    'settings: Settings, custom: CustomLayout',
)
replace_once(
    touch,
    '''        Self {
            default_stick: (movement_x, movement_y),
            stick_radius,
            dpad_center: (movement_x, movement_y),
            dpad_radius,
            buttons,
            width: w,
            height: h,
            settings,
        }
''',
    '''        let mut layout = Self {
            default_stick: (movement_x, movement_y),
            stick_radius,
            dpad_center: (movement_x, movement_y),
            dpad_radius,
            buttons,
            width: w,
            height: h,
            settings,
        };
        custom.apply(&mut layout, w, h);
        layout
''',
    'custom.apply(&mut layout, w, h);',
)
replace_once(
    touch,
    '''    pressed: Vec<Control>,
    settings: Settings,
}
''',
    '''    pressed: Vec<Control>,
    settings: Settings,
    custom: CustomLayout,
    editor: TouchEditor,
}
''',
    'custom: CustomLayout,',
)
replace_once(
    touch,
    '''            pressed: Vec::new(),
            settings: Settings::load(),
''',
    '''            pressed: Vec::new(),
            settings: Settings::load(),
            custom: CustomLayout::load(),
            editor: TouchEditor::default(),
''',
    'custom: CustomLayout::load()',
)
# Every runtime Layout uses the persisted custom geometry.
text = touch.read_text()
text = text.replace(
    'Layout::new((batch.width, batch.height), settings)',
    'Layout::new((batch.width, batch.height), settings, self.touch_ui.custom)',
)
touch.write_text(text)

# Editor intercepts raw Android touch before the pause screen converts it to a mouse.
replace_once(
    touch,
    '''    ) {
        // F1 may hide the HUD, but it must not turn the touchscreen into a
''',
    '''    ) {
        if self.touch_editor_event(touch) {
            return;
        }
        // F1 may hide the HUD, but it must not turn the touchscreen into a
''',
    'if self.touch_editor_event(touch)',
)

# Runtime control rendering uses per-control opacity.
replace_once(
    touch,
    '''                layout.dpad_radius,
                self.touch_ui.stick,
            ),
''',
    '''                layout.dpad_radius,
                self.touch_ui.stick,
                self.touch_ui.custom.opacity(EditorTarget::Movement),
            ),
''',
    'custom.opacity(EditorTarget::Movement),',
)
replace_once(
    touch,
    'touch_icons::joystick(batch, atlas, sx, sy, layout.stick_radius, self.touch_ui.stick);',
    'touch_icons::joystick(batch, atlas, sx, sy, layout.stick_radius, self.touch_ui.stick, self.touch_ui.custom.opacity(EditorTarget::Movement));',
    'layout.stick_radius, self.touch_ui.stick, self.touch_ui.custom.opacity(EditorTarget::Movement)',
)
replace_once(
    touch,
    'touch_icons::button(batch, atlas, icon, *cx, *cy, *r, down);',
    'touch_icons::button(batch, atlas, icon, *cx, *cy, *r, down, self.touch_ui.custom.opacity(EditorTarget::Control(*control)));',
    'custom.opacity(EditorTarget::Control(*control))',
)

# Full live editor. It deliberately uses touch, not pointer emulation, so drag
# remains one-to-one with the player's finger.
replace_once(
    touch,
    '''    pub(super) fn set_soft_keyboard(&self, show: bool) {
''',
    '''    pub(super) fn touch_editor_active(&self) -> bool {
        self.touch_ui.editor.active
    }

    pub(super) fn touch_begin_editor(&mut self) {
        self.release_touch_inputs();
        self.touch_ui.editor.active = true;
        self.touch_ui.editor.finger = None;
    }

    fn editor_target_geometry(layout: &Layout, target: EditorTarget) -> ((f32, f32), f32) {
        match target {
            EditorTarget::Movement => {
                let r = if layout.settings.preset == Preset::DpadTap { layout.dpad_radius } else { layout.stick_radius };
                (layout.default_stick, r)
            }
            EditorTarget::Control(control) => layout
                .buttons
                .iter()
                .find(|(c, _, _)| *c == control)
                .map(|(_, centre, r)| (*centre, *r))
                .unwrap_or(((layout.width * 0.5, layout.height * 0.5), 20.0)),
        }
    }

    fn editor_target_at(layout: &Layout, pos: (f64, f64)) -> Option<EditorTarget> {
        if let Some(control) = layout.button_at(pos) {
            return Some(EditorTarget::Control(control));
        }
        let (centre, r) = Self::editor_target_geometry(layout, EditorTarget::Movement);
        let dx = pos.0 - f64::from(centre.0);
        let dy = pos.1 - f64::from(centre.1);
        (dx * dx + dy * dy <= f64::from(r * 1.25).powi(2)).then_some(EditorTarget::Movement)
    }

    fn ensure_editor_placement(&mut self, target: EditorTarget, layout: &Layout) -> Placement {
        if let Some(p) = self.touch_ui.custom.placement(target) {
            return p;
        }
        let (centre, _) = Self::editor_target_geometry(layout, target);
        let p = Placement {
            x: centre.0 / layout.width.max(1.0),
            y: centre.1 / layout.height.max(1.0),
            scale: 1.0,
            opacity: 1.0,
        };
        self.touch_ui.custom.set(target, p);
        p
    }

    fn editor_toolbar(width: f32) -> [[f32; 4]; 6] {
        let middle = width * 0.5;
        [
            [8.0, 8.0, 66.0, 21.0],                   // reset
            [middle - 122.0, 8.0, 54.0, 21.0],       // size -
            [middle - 64.0, 8.0, 54.0, 21.0],        // size +
            [middle + 10.0, 8.0, 70.0, 21.0],        // opacity -
            [middle + 84.0, 8.0, 70.0, 21.0],        // opacity +
            [width - 74.0, 8.0, 66.0, 21.0],         // done
        ]
    }

    fn editor_adjust_selected(&mut self, layout: &Layout, scale_delta: f32, opacity_delta: f32) {
        let Some(target) = self.touch_ui.editor.selected else { return };
        let mut p = self.ensure_editor_placement(target, layout);
        p.scale = (p.scale + scale_delta).clamp(0.55, 1.80);
        p.opacity = (p.opacity + opacity_delta).clamp(0.20, 1.0);
        self.touch_ui.custom.set(target, p);
    }

    fn touch_editor_event(&mut self, touch: &winit::event::Touch) -> bool {
        if !self.touch_ui.editor.active {
            return false;
        }
        let batch = HudBatch::new(self.touch_pixels());
        let settings = self.touch_ui.settings;
        let layout = Layout::new((batch.width, batch.height), settings, self.touch_ui.custom);
        let scale = f64::from(batch.scale);
        let pos = (touch.location.x / scale, touch.location.y / scale);
        let toolbar = Self::editor_toolbar(batch.width);
        let inside = |rect: [f32; 4]| {
            pos.0 >= f64::from(rect[0])
                && pos.0 < f64::from(rect[0] + rect[2])
                && pos.1 >= f64::from(rect[1])
                && pos.1 < f64::from(rect[1] + rect[3])
        };

        match touch.phase {
            TouchPhase::Started => {
                if inside(toolbar[0]) {
                    self.touch_ui.custom = CustomLayout::default();
                    self.touch_ui.editor.selected = None;
                    let _ = self.touch_ui.custom.save();
                    return true;
                }
                if inside(toolbar[5]) {
                    let _ = self.touch_ui.custom.save();
                    self.touch_ui.editor.active = false;
                    self.touch_ui.editor.finger = None;
                    return true;
                }
                if inside(toolbar[1]) { self.editor_adjust_selected(&layout, -0.10, 0.0); return true; }
                if inside(toolbar[2]) { self.editor_adjust_selected(&layout, 0.10, 0.0); return true; }
                if inside(toolbar[3]) { self.editor_adjust_selected(&layout, 0.0, -0.10); return true; }
                if inside(toolbar[4]) { self.editor_adjust_selected(&layout, 0.0, 0.10); return true; }

                if let Some(target) = Self::editor_target_at(&layout, pos) {
                    let (centre, _) = Self::editor_target_geometry(&layout, target);
                    self.ensure_editor_placement(target, &layout);
                    self.touch_ui.editor.selected = Some(target);
                    self.touch_ui.editor.finger = Some(touch.id);
                    self.touch_ui.editor.drag_offset = (pos.0 - f64::from(centre.0), pos.1 - f64::from(centre.1));
                } else {
                    self.touch_ui.editor.selected = None;
                }
            }
            TouchPhase::Moved => {
                if self.touch_ui.editor.finger == Some(touch.id)
                    && let Some(target) = self.touch_ui.editor.selected
                {
                    let mut p = self.ensure_editor_placement(target, &layout);
                    p.x = ((pos.0 - self.touch_ui.editor.drag_offset.0) as f32 / batch.width.max(1.0)).clamp(0.03, 0.97);
                    p.y = ((pos.1 - self.touch_ui.editor.drag_offset.1) as f32 / batch.height.max(1.0)).clamp(0.04, 0.96);
                    self.touch_ui.custom.set(target, p);
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if self.touch_ui.editor.finger == Some(touch.id) {
                    self.touch_ui.editor.finger = None;
                    let _ = self.touch_ui.custom.save();
                }
            }
        }
        true
    }

    pub(super) fn draw_touch_editor(&mut self, batch: &mut HudBatch) {
        let settings = self.touch_ui.settings;
        let layout = Layout::new((batch.width, batch.height), settings, self.touch_ui.custom);
        let atlas = &self.gui.atlas;
        let selected = self.touch_ui.editor.selected;

        // A narrow editor bar keeps the world/pause background visible, as in
        // Bedrock's control customizer.
        batch.fill(atlas, 0.0, 0.0, batch.width, 37.0, rgba(0, 0, 0, 188));
        let toolbar = Self::editor_toolbar(batch.width);
        let labels = ["Reset", "Size -", "Size +", "Opacity -", "Opacity +", "Done"];
        for (rect, label) in toolbar.into_iter().zip(labels) {
            batch.fill(atlas, rect[0], rect[1], rect[2], rect[3], rgba(42, 44, 50, 232));
            let tw = HudBatch::text_width(atlas, label);
            batch.text(atlas, label, rect[0] + (rect[2] - tw) * 0.5, rect[1] + 7.0, u32::MAX, true);
        }
        let name = selected.map_or("Tap a control to select", EditorTarget::label);
        let tw = HudBatch::text_width(atlas, name);
        batch.text(atlas, name, (batch.width - tw) * 0.5, 34.0, rgba(232, 232, 235, 255), true);

        let movement_opacity = self.touch_ui.custom.opacity(EditorTarget::Movement)
            .max(if selected == Some(EditorTarget::Movement) { 0.70 } else { 0.20 });
        match settings.preset {
            Preset::DpadTap => touch_icons::dpad(
                batch, atlas, layout.dpad_center.0, layout.dpad_center.1,
                layout.dpad_radius, [0.0, 0.0], movement_opacity,
            ),
            Preset::JoystickTap | Preset::JoystickCrosshair => touch_icons::joystick(
                batch, atlas, layout.default_stick.0, layout.default_stick.1,
                layout.stick_radius, [0.0, 0.0], movement_opacity,
            ),
        }

        for (control, (cx, cy), r) in &layout.buttons {
            let target = EditorTarget::Control(*control);
            let icon = match control {
                Control::Jump => touch_icons::Icon::Jump,
                Control::Sneak => touch_icons::Icon::Sneak,
                Control::Sprint => touch_icons::Icon::Sprint,
                Control::Attack => touch_icons::Icon::Attack,
                Control::Use => touch_icons::Icon::Use,
                Control::Inventory => touch_icons::Icon::Inventory,
                Control::Chat => touch_icons::Icon::Chat,
                Control::Pause => touch_icons::Icon::Pause,
                Control::Perspective => touch_icons::Icon::Perspective,
            };
            let is_selected = selected == Some(target);
            let opacity = self.touch_ui.custom.opacity(target).max(if is_selected { 0.70 } else { 0.20 });
            touch_icons::button(batch, atlas, icon, *cx, *cy, *r, is_selected, opacity);
        }
        batch.text(
            atlas,
            "Drag controls · select then change size/opacity · Reset restores this layout",
            10.0,
            batch.height - 12.0,
            rgba(230, 230, 232, 230),
            true,
        );
    }

    pub(super) fn set_soft_keyboard(&self, show: bool) {
''',
    'pub(super) fn touch_editor_active(&self)',
)

# ----- settings panel integration ---------------------------------------
replace_once(
    settings_ui,
    '''    pub(super) fn draw_android_touch_settings_entry(&mut self, batch: &mut HudBatch) {
        let atlas = &self.gui.atlas;
''',
    '''    pub(super) fn draw_android_touch_settings_entry(&mut self, batch: &mut HudBatch) {
        if self.touch_editor_active() {
            self.draw_touch_editor(batch);
            return;
        }
        let atlas = &self.gui.atlas;
''',
    'if self.touch_editor_active()',
)
replace_once(
    settings_ui,
    '''            "Customize",
            false,
''',
    '''            "Customize",
            true,
''',
    '"Customize",\n            true,',
)
replace_once(
    settings_ui,
    '''        if hovered(self.cursor, [x + 20.0 + bw * 2.0, actions_y, bw, 20.0]) {
''',
    '''        if hovered(self.cursor, [x + 14.0 + bw, actions_y, bw, 20.0]) {
            {
                let mut state = lock_state();
                state.open = false;
                state.status = None;
            }
            self.touch_begin_editor();
            self.audio.play_button_click();
            return true;
        }
        if hovered(self.cursor, [x + 20.0 + bw * 2.0, actions_y, bw, 20.0]) {
''',
    'self.touch_begin_editor();',
)
replace_once(
    settings_ui,
    '            "Customize layout is the next parity layer; disabled until drag/resize is complete.",\n',
    '            "Customize opens a live drag / size / opacity editor over the game.",\n',
    'Customize opens a live drag / size / opacity editor',
)

print("patched Horizon Android Bedrock touch layout editor")
