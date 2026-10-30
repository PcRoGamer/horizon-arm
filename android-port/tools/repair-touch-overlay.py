#!/usr/bin/env python3
"""Repair/scoped-normalize Android touch HUD wrapping after legacy patchers.

The legacy hotbar-only patch used global string anchors. Once the live layout
editor was added, its `match settings.preset` became the first match and the
opening/closing braces could span two different Game methods. This script
removes those legacy markers and reinstalls the wrapper only inside
Game::draw_touch_controls.

It is intentionally idempotent and runs only in the disposable Android build
tree; desktop/upstream sources remain untouched.
"""

from pathlib import Path
import sys

root = Path(sys.argv[1])
touch = root / "crates/horizon-client/src/game/touch.rs"
text = touch.read_text()

BEGIN = "// Horizon Android hotbar-only movement HUD begin"
END = "// Horizon Android hotbar-only movement HUD end"

# Undo any legacy globally-anchored wrapping first. These exact strings are
# emitted by patch-touch-parity-options.py.
text = text.replace(
    f"        {BEGIN}\n        if !settings.hotbar_only {{\n",
    "",
)
text = text.replace(
    f"        }} {END}\n",
    "",
)

method = "    pub(super) fn draw_touch_controls(&mut self, batch: &mut HudBatch) {\n"
start = text.find(method)
if start < 0:
    raise SystemExit("repair-touch-overlay: draw_touch_controls method not found")

# Limit both anchors to draw_touch_controls. The next top-level helper marks the
# end of the Game impl in the Android touch module.
limit = text.find("\nfn look_at", start)
if limit < 0:
    raise SystemExit("repair-touch-overlay: fn look_at boundary not found")

body = text[start:limit]
match_anchor = "        match settings.preset {\n"
match_pos = body.find(match_anchor)
if match_pos < 0:
    raise SystemExit("repair-touch-overlay: runtime preset match not found")

buttons_anchor = "        for (control, (cx, cy), r) in &layout.buttons {\n"
buttons_pos = body.find(buttons_anchor, match_pos)
if buttons_pos < 0:
    raise SystemExit("repair-touch-overlay: runtime button loop not found")

# The movement match ends immediately before the runtime button loop. Insert a
# scoped wrapper rather than relying on brace-shaped global anchors.
body = (
    body[:match_pos]
    + f"        {BEGIN}\n        if !settings.hotbar_only {{\n"
    + body[match_pos:buttons_pos]
    + f"        }} {END}\n\n"
    + body[buttons_pos:]
)
text = text[:start] + body + text[limit:]

touch.write_text(text)
print("repaired Horizon Android touch impl scope")
