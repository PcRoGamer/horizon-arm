#!/usr/bin/env python3
"""Repair/scoped-normalize Android touch HUD wrapping after legacy patchers.

The legacy hotbar-only patch used global string anchors. Once the live layout
editor was added, its `match settings.preset` became the first match and the
opening/closing braces could span two different Game methods. This script
removes those legacy markers and reinstalls the wrapper only inside
Game::draw_touch_controls.

It also installs an Android-only pose around Horizon's normal hotbar draw calls:
the actual upstream hotbar stays authoritative, but is 25% larger and raised
24 GUI pixels on phones. This keeps renderer/shader/item parity while making the
mobile target comfortably tappable.

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
if BEGIN not in body:
    body = (
        body[:match_pos]
        + f"        {BEGIN}\n        if !settings.hotbar_only {{\n"
        + body[match_pos:buttons_pos]
        + f"        }} {END}\n\n"
        + body[buttons_pos:]
    )
text = text[:start] + body + text[limit:]
touch.write_text(text)

# Enlarge/elevate Horizon's own hotbar rather than maintaining a second mobile
# hotbar implementation. The pose wraps selection, items, offhand and attack
# indicator together, so all upstream hotbar features remain intact.
hud = root / "crates/horizon-client-render/src/hud_layout.rs"
h = hud.read_text()
MARK = "Horizon Android mobile hotbar transform"
if MARK not in h:
    fn_start = h.find("pub fn hotbar(")
    if fn_start < 0:
        raise SystemExit("repair-touch-overlay: hud_layout::hotbar missing")

    brace = h.find("{", fn_start)
    if brace < 0:
        raise SystemExit("repair-touch-overlay: hotbar opening brace missing")
    depth = 0
    fn_end = None
    for i in range(brace, len(h)):
        c = h[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                fn_end = i
                break
    if fn_end is None:
        raise SystemExit("repair-touch-overlay: hotbar closing brace missing")

    body = h[fn_start:fn_end]
    center = "    let center = w / 2;\n"
    if center not in body:
        raise SystemExit("repair-touch-overlay: hotbar center anchor changed")
    push = center + '''    #[cfg(target_os = "android")]
    out.push(GuiDraw::Push(vec![
        // Horizon Android mobile hotbar transform: larger and lifted away
        // from the gesture/navigation edge while preserving upstream draws.
        PoseOp::Translate(0.0, -24.0),
        PoseOp::Translate(center as f32, h as f32),
        PoseOp::Scale(1.25, 1.25),
        PoseOp::Translate(-(center as f32), -(h as f32)),
    ]));
'''
    body = body.replace(center, push, 1)

    # The function's final expression is `out`; place Pop immediately before it.
    tail = "    out\n"
    tail_at = body.rfind(tail)
    if tail_at < 0:
        raise SystemExit("repair-touch-overlay: hotbar return anchor changed")
    pop = '''    #[cfg(target_os = "android")]
    out.push(GuiDraw::Pop);
'''
    body = body[:tail_at] + pop + body[tail_at:]
    h = h[:fn_start] + body + h[fn_end:]
    hud.write_text(h)

print("repaired Horizon Android touch impl scope and mobile hotbar transform")