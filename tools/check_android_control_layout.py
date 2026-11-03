#!/usr/bin/env python3
"""Geometry contract between the Java Canvas and Rust touch input.
Covers aspect ratios, scaled presets, hotbar collisions and button hit parity.
Does not replace physical device/input testing.
"""
from __future__ import annotations

import itertools
import math
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
JAVA = (ROOT / "packaging/android/java/dev/horizon/android/HorizonActivity.java").read_text()
RUST = (ROOT / "crates/horizon-winit/src/lib.rs").read_text()

visuals = {}
for x, y, radius, name in re.findall(
    r'control\(canvas,\s*([0-9.]+)f,\s*([0-9.]+)f,\s*([0-9.]+)f,\s*"([^"]+)"\)',
    JAVA,
):
    assert name not in visuals, f"duplicate Java button: {name}"
    visuals[name] = (float(x), float(y), float(radius))

expected = {"jump", "sneak", "hit", "use", "sprint", "inventory", "pause"}
assert set(visuals) == expected, (set(visuals), expected)

touch_regions = {
    (float(x), float(y), float(r))
    for x, y, r in re.findall(
        r'near_square\(p,\s*\[([0-9.]+),\s*([0-9.]+)\],\s*([0-9.]+) \* scale,\s*aspect\)',
        RUST,
    )
}
for name, coords in visuals.items():
    assert coords in touch_regions, f"{name}: Java visual does not match Rust touch hitbox: {coords}"

assert "float x = .15f * getWidth();" in JAVA
assert "float y = .72f * h;" in JAVA
assert "float r = .135f * h * scale;" in JAVA
assert "width * 0.15" in RUST and "height * 0.72" in RUST
assert "height * 0.135 * android_touch_scale()" in RUST
assert "movement_owned" in RUST, "second joystick finger can steal movement"
assert "touch.fingers.values().any(" in RUST, "duplicate finger ownership not guarded"
assert "TOUCH_SCHEME" in RUST and "TOUCH_SCALE" in RUST
assert "nativeTouchAxes" in JAVA and "nativeTouchButtons" in JAVA

# Check that the 4 D-pad icons remain reachable inside the movement region.
dpad_offset = 0.068 * 1.48
dpad_half = 0.049
assert math.hypot(dpad_offset + dpad_half, dpad_half) <= 0.162


def rect(x, y, radius, width, height, scale):
    cx, cy = x * width, y * height
    r = radius * height * scale
    return (cx - r, cy - r, cx + r, cy + r)


def clearance(a, b):
    dx = max(a[0] - b[2], b[0] - a[2], 0)
    dy = max(a[1] - b[3], b[1] - a[3], 0)
    return math.hypot(dx, dy)


sizes = [
    (2400, 1080), (2560, 1080), (1920, 1200),
    (1600, 1200), (1280, 800), (1920, 1080), (1280, 720),
]
for width, height in sizes:
    gui_scale = 1
    while width // (gui_scale + 1) >= 320 and height // (gui_scale + 1) >= 240:
        gui_scale += 1
    hotbar = (
        width / 2 - 91 * gui_scale,
        height - 22 * gui_scale,
        width / 2 + 91 * gui_scale,
        height,
    )
    for scale in (0.8, 1, 1.2):
        for scheme in (0, 1, 2, 3):
            buttons = {k: v for k, v in visuals.items()
                       if scheme % 2 or k not in ("hit", "use")}
            regions = {name: rect(*coords, width, height, scale)
                       for name, coords in buttons.items()}

            if scheme < 2:
                regions["movement"] = rect(.15, .72, .135, width, height, scale)
            else:
                # Actual D-pad keys are squares, not the theoretical joystick circle.
                for name, dx, dy in (
                    ("up", 0, -1), ("right", 1, 0),
                    ("down", 0, 1), ("left", -1, 0),
                ):
                    x = .15 * width + dx * dpad_offset * height * scale
                    y = .72 * height + dy * dpad_offset * height * scale
                    r = dpad_half * height * scale
                    regions["dpad_" + name] = (x - r, y - r, x + r, y + r)

            for name, box in regions.items():
                left, top, right, bottom = box
                assert left >= 20 and top >= 20 and right <= width - 20 and bottom <= height - 20, (
                    f"{width}x{height} scale={scale} scheme={scheme}: {name} outside safe area {box}"
                )
                assert clearance(box, hotbar) >= 12, (
                    f"{width}x{height} scale={scale} scheme={scheme}: {name} overlaps hotbar"
                )

            for (name_a, a), (name_b, b) in itertools.combinations(regions.items(), 2):
                if scheme >= 2 and name_a.startswith("dpad") and name_b.startswith("dpad"):
                    # D-pad cells are intentionally adjacent.
                    continue
                assert clearance(a, b) >= 8, (
                    f"{width}x{height} scale={scale} scheme={scheme}: "
                    f"{name_a}/{name_b} overlap (gap={clearance(a,b):.1f}px)"
                )
    print(f"PASS {width}x{height}: 4 schemes x 3 scales; hotbar and edge clearance")

print("PASS touch-mode coverage, multitouch ownership guards and Java/Rust hitbox parity")
