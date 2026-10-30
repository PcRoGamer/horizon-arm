#!/usr/bin/env python3
"""Android-only Pocket/Bedrock HUD presentation.

Horizon keeps the vanilla Java HUD extraction/parity code untouched in Git.
The APK build applies these presentation-only changes after checkout so Android
gets mobile placement without weakening desktop/vanilla parity tests.
"""

from pathlib import Path
import sys

root = Path(sys.argv[1])
hud = root / "crates/horizon-client/src/hud_layout.rs"


def replace_once(old: str, new: str, marker: str) -> None:
    text = hud.read_text()
    if marker in text:
        return
    if old not in text:
        raise SystemExit(f"patch-bedrock-hud: anchor not found: {old[:120]!r}")
    hud.write_text(text.replace(old, new, 1))


# Pocket UI keeps the hotbar visually light/transparent.  Items and the
# selected-slot frame still use Horizon's normal Minecraft sprites.
replace_once(
    '    out.push(sprite("hud/hotbar", center - 91, h - 22, 182, 22));\n',
    '    #[cfg(not(target_os = "android"))]\n'
    '    out.push(sprite("hud/hotbar", center - 91, h - 22, 182, 22));\n'
    '    // Android Pocket UI intentionally has no opaque hotbar rail.\n',
    'Android Pocket UI intentionally has no opaque hotbar rail',
)

# Bedrock tap modes do not show a crosshair.  Crosshair mode does.  Read the
# same tiny Android touch config once per process; changing modes is applied on
# the next game start, which matches the current launcherless settings model.
replace_once(
    '''pub fn crosshair(w: i32, h: i32, attack: Option<f32>, full: bool) -> Vec<GuiDraw> {
    crosshair_draws(w, h, attack, full)
        .into_iter()
        .map(crosshair_blend)
        .collect()
}
''',
    '''#[cfg(target_os = "android")]
fn android_touch_crosshair() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let configured = std::env::var("HORIZON_TOUCH_PRESET")
            .ok()
            .or_else(|| std::fs::read_to_string("horizon-touch.txt").ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let simple = configured.trim();
        matches!(simple, "classic" | "crosshair")
            || configured.lines().any(|line| {
                line.split_once('=').is_some_and(|(key, value)| {
                    matches!(key.trim(), "mode" | "control_mode")
                        && matches!(value.trim(), "crosshair" | "joystick_crosshair" | "joystick & aim crosshair")
                })
            })
    })
}

pub fn crosshair(w: i32, h: i32, attack: Option<f32>, full: bool) -> Vec<GuiDraw> {
    #[cfg(target_os = "android")]
    if !android_touch_crosshair() {
        return Vec::new();
    }
    crosshair_draws(w, h, attack, full)
        .into_iter()
        .map(crosshair_blend)
        .collect()
}
''',
    'fn android_touch_crosshair()',
)

# Bedrock Pocket UI places player status in the upper corners.  Keep Horizon's
# exact heart/food/armour/air extraction and animation logic; only change its
# anchors on Android.  Extra heart rows grow downward from a safe top margin.
replace_once(
    '''        let (x_left, x_right, y_base) = (w / 2 - 91, w / 2 + 91, h - 39);
        let max_health = i.max_health.max(old.max(current) as f32);
        let absorption = ceil(i.absorption);
        let rows = ceil((max_health + absorption as f32) / 2.0 / 10.0);
        let row_height = (10 - (rows - 2)).max(3);
        let mut y_air = y_base - 10;
''',
    '''        let max_health = i.max_health.max(old.max(current) as f32);
        let absorption = ceil(i.absorption);
        let rows = ceil((max_health + absorption as f32) / 2.0 / 10.0);
        let row_height = (10 - (rows - 2)).max(3);
        #[cfg(not(target_os = "android"))]
        let (x_left, x_right, y_base) = (w / 2 - 91, w / 2 + 91, h - 39);
        #[cfg(target_os = "android")]
        let (x_left, x_right, y_base) = {
            let safe = 12;
            let top = 13;
            let left_rows = (rows - 1) * row_height + if i.armor > 0 { 10 } else { 0 };
            (safe, w - safe, top + left_rows)
        };
        #[cfg(not(target_os = "android"))]
        let mut y_air = y_base - 10;
        #[cfg(target_os = "android")]
        let mut y_air = y_base;
''',
    'let left_rows = (rows - 1) * row_height',
)

# On Java the air row stacks upward from the bottom-right food row.  Pocket UI
# has free space below the top-right food row, so stack oxygen downward instead.
replace_once(
    '''            y_air -= 10;
            self.right_height += 10;
''',
    '''            #[cfg(not(target_os = "android"))]
            { y_air -= 10; }
            #[cfg(target_os = "android")]
            { y_air += 10; }
            self.right_height += 10;
''',
    '{ y_air += 10; }',
)

# Mount hearts follow the same top-right Pocket UI anchor.
replace_once(
    '''    let x_right = w / 2 + 91;
    let mut yo = h - 39;
''',
    '''    #[cfg(not(target_os = "android"))]
    let x_right = w / 2 + 91;
    #[cfg(not(target_os = "android"))]
    let mut yo = h - 39;
    #[cfg(target_os = "android")]
    let x_right = w - 12;
    #[cfg(target_os = "android")]
    let mut yo = 23;
''',
    'let mut yo = 23;',
)

print("patched Horizon Android Pocket HUD anchors")
