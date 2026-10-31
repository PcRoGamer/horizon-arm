#!/usr/bin/env python3
"""Add the minimal Game state owned by the Android touch module.

The implementation itself lives in the additive touch.rs overlay. Upstream's
Game only needs two Android-gated fields and their defaults; desktop layout and
behaviour are untouched.
"""
from pathlib import Path
import sys

root = Path(sys.argv[1])
game = root / "crates/horizon-client/src/game.rs"


def replace_once(old: str, new: str, marker: str) -> None:
    text = game.read_text()
    if marker in text:
        return
    if old not in text:
        raise SystemExit(f"patch-android-touch-state: anchor not found: {old[:140]!r}")
    game.write_text(text.replace(old, new, 1))


replace_once(
    '''    /// MouseHandler's accumulated look and wheel state.
    mouse: MouseLook,
    /// An agent moved the mouse (`mouse-move`) since the pointer was last
''',
    '''    /// MouseHandler's accumulated look and wheel state.
    mouse: MouseLook,
    /// Android's active menu pointer finger.
    #[cfg(target_os = "android")]
    touch: Option<(u64, (f64, f64))>,
    /// Android's independent movement/look/action fingers and HUD state.
    #[cfg(target_os = "android")]
    touch_ui: touch::Touch,
    /// An agent moved the mouse (`mouse-move`) since the pointer was last
''',
    'touch_ui: touch::Touch',
)

replace_once(
    '''            fullscreen: horizon_client::window::FullscreenState::new(false),
            mouse: MouseLook::default(),
            agent_pointer: false,
''',
    '''            fullscreen: horizon_client::window::FullscreenState::new(false),
            mouse: MouseLook::default(),
            #[cfg(target_os = "android")]
            touch: None,
            #[cfg(target_os = "android")]
            touch_ui: touch::Touch::default(),
            agent_pointer: false,
''',
    'touch_ui: touch::Touch::default()',
)

print("patched Horizon Android touch state")
