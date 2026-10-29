#!/usr/bin/env bash
# Android-only source hooks for the launcherless management UI.
#
# Horizon's game crate stays upstream-shaped on desktop. The Android APK build
# adds a tiny module declaration plus pause/input hooks so the mobile-specific
# manager can live in crates/horizon-client/src/game/mobile_ui.rs without
# spreading cfg branches through the game.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=tools/env.sh
source "$REPO/tools/env.sh"

python3 - "$HORIZON_SRC" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1])
game = root / "crates/horizon-client/src/game.rs"
screens = root / "crates/horizon-client/src/game/screens.rs"


def replace_once(path: Path, old: str, new: str, marker: str) -> None:
    text = path.read_text()
    if marker in text:
        return
    if old not in text:
        raise SystemExit(f"patch-horizon-mobile: anchor not found in {path}: {old[:80]!r}")
    path.write_text(text.replace(old, new, 1))

# Compile the Android-only UI module next to the touch module.
replace_once(
    game,
    '#[cfg(target_os = "android")]\nmod touch;\n',
    '#[cfg(target_os = "android")]\nmod touch;\n#[cfg(target_os = "android")]\nmod mobile_ui; // Horizon Android launcherless manager\n',
    'mod mobile_ui; // Horizon Android launcherless manager',
)

# Name editing needs first refusal on keyboard events; otherwise the vanilla
# title/chat handlers would consume the same Enter/Backspace/text.
replace_once(
    game,
    '        #[cfg(target_os = "android")]\n        if let WindowEvent::Touch(touch) = &event {\n            self.touch(event_loop, id, touch);\n            return;\n        }\n',
    '        #[cfg(target_os = "android")]\n        if let WindowEvent::KeyboardInput { event, .. } = &event\n            && self.android_manager_key(event)\n        {\n            return;\n        }\n        #[cfg(target_os = "android")]\n        if let WindowEvent::Touch(touch) = &event {\n            self.touch(event_loop, id, touch);\n            return;\n        }\n',
    'self.android_manager_key(event)',
)

# The manager is a pause overlay, not a replacement pause screen.
replace_once(
    screens,
    '            ui::tooltip(batch, atlas, &lines, self.cursor);\n        }\n    }\n    pub(super) fn pause_click(&mut self) -> Result<()> {\n',
    '            ui::tooltip(batch, atlas, &lines, self.cursor);\n        }\n        #[cfg(target_os = "android")]\n        self.draw_android_pause_controls(batch);\n    }\n    pub(super) fn pause_click(&mut self) -> Result<()> {\n        #[cfg(target_os = "android")]\n        if self.android_pause_click() {\n            return Ok(());\n        }\n',
    'self.draw_android_pause_controls(batch)',
)

print("patched Horizon Android mobile UI hooks")
PY
