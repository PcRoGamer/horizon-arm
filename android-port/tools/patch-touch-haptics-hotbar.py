#!/usr/bin/env python3
"""Add hotbar tap/swipe haptics after all Bedrock layout/parity patches."""

from pathlib import Path
import sys

root = Path(sys.argv[1])
touch = root / "crates/horizon-client/src/game/touch.rs"
text = touch.read_text()

start_marker = "// Horizon Android hotbar press haptic"
if start_marker not in text:
    old = '''                let gesture = if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                    self.selected = slot;
                    Gesture::Hotbar
'''
    new = '''                // Horizon Android hotbar press haptic
                let gesture = if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                    if slot != self.selected && self.touch_ui.settings.button_haptics {
                        crate::android::haptic(crate::android::Haptic::Tap);
                    }
                    self.selected = slot;
                    Gesture::Hotbar
'''
    if old not in text:
        raise SystemExit("patch-touch-haptics-hotbar: hotbar press anchor not found")
    text = text.replace(old, new, 1)

swipe_marker = "// Horizon Android hotbar swipe haptic"
if swipe_marker not in text:
    old = '''                    Gesture::Hotbar => {
                        if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                            self.selected = slot;
                        }
'''
    new = '''                    // Horizon Android hotbar swipe haptic
                    Gesture::Hotbar => {
                        if let Some(slot) = hotbar_slot(pos, (batch.width, batch.height)) {
                            if slot != self.selected && self.touch_ui.settings.button_haptics {
                                crate::android::haptic(crate::android::Haptic::Tap);
                            }
                            self.selected = slot;
                        }
'''
    if old not in text:
        raise SystemExit("patch-touch-haptics-hotbar: hotbar swipe anchor not found")
    text = text.replace(old, new, 1)

touch.write_text(text)
print("patched Horizon Android hotbar press/swipe haptics")
