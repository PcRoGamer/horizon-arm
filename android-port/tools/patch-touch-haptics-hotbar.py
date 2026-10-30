#!/usr/bin/env python3
"""Complete hotbar-swipe haptics after the main Android haptics patch."""

from pathlib import Path
import sys

root = Path(sys.argv[1])
touch = root / "crates/horizon-client/src/game/touch.rs"
text = touch.read_text()

marker = "// Horizon Android hotbar swipe haptic"
if marker not in text:
    old = '''// Horizon Android hotbar haptic
                    Gesture::Hotbar => {
                        if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                            self.selected = slot;
                        }
'''
    new = '''// Horizon Android hotbar swipe haptic
                    Gesture::Hotbar => {
                        if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                            if slot != self.selected && self.touch_ui.settings.button_haptics {
                                crate::android::haptic(crate::android::Haptic::Tap);
                            }
                            self.selected = slot;
                        }
'''
    if old not in text:
        raise SystemExit("patch-touch-haptics-hotbar: hotbar move anchor not found")
    touch.write_text(text.replace(old, new, 1))

print("patched Horizon Android hotbar swipe haptics")
