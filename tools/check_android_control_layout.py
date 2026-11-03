#!/usr/bin/env python3
"""Static visual-layout guard for the Android native overlay and Rust hitboxes.

The checks use Java Canvas geometry and Horizon's real GUI scaling. They
cannot replace inspection on a physical device.
"""
from __future__ import annotations

import math
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
JAVA = (ROOT / "packaging/android/java/dev/horizon/android/HorizonActivity.java").read_text()
RUST = (ROOT / "crates/horizon-winit/src/lib.rs").read_text()

circles = {}
for x, y, radius, label in re.findall(
    r'circle\(canvas,\s*([0-9.]+)f,\s*([0-9.]+)f,\s*([0-9.]+)f,\s*"([^"]+)"\)',
    JAVA,
):
    if label in circles:
        raise AssertionError(f"duplicate Java button: {label}")
    circles[label] = tuple(map(float, (x, y, radius)))

expected = {"JUMP", "SNEAK", "HIT", "USE", "INV", "II"}
assert expected == set(circles), f"missing or unexpected buttons: {set(circles) ^ expected}"

rust_regions = {
    tuple(map(float, values))
    for values in re.findall(
        r'near\(p,\s*\[([0-9.]+),\s*([0-9.]+)\],\s*([0-9.]+),\s*aspect\)',
        RUST,
    )
}
for label, coordinate in circles.items():
    assert coordinate in rust_regions, f"{label}: Java circle != Rust hit region: {coordinate}"

# The thumbstick is drawn twice at this centre and handled by the same
# fixed-centre movement math in the Rust adapter.
assert "0.14f, 0.75f, 0.16f, null" in JAVA
assert "0.14f, 0.75f, 0.055f, null" in JAVA
assert "width * 0.14" in RUST and "height * 0.75" in RUST

regions = {"JOYSTICK": (0.14, 0.75, 0.16), **circles}


def distance_to_rect(px, py, rect):
    left, top, right, bottom = rect
    nx = min(max(px, left), right)
    ny = min(max(py, top), bottom)
    return math.hypot(px - nx, py - ny)


for width, height in [(2400, 1080), (2560, 1080), (1920, 1200), (1600, 1200), (1280, 800)]:
    # Same algorithm as HudBatch::new. Real hotbar is 182 GUI pixels wide.
    scale = 1
    while width // (scale + 1) >= 320 and height // (scale + 1) >= 240:
        scale += 1
    bar = (
        width / 2 - 91 * scale,
        height - 22 * scale,
        width / 2 + 91 * scale,
        height,
    )

    for label, (xn, yn, rn) in regions.items():
        x, y, r = xn * width, yn * height, rn * height
        edge_margin = min(x - r, y - r, width - x - r, height - y - r)
        bar_clearance = distance_to_rect(x, y, bar) - r
        assert edge_margin >= 24, (
            f"{width}x{height}: {label} too close to screen edge: {edge_margin:.1f}px"
        )
        assert bar_clearance >= 24, (
            f"{width}x{height}: {label} overlaps/approaches hotbar: {bar_clearance:.1f}px"
        )

    items = list(regions.items())
    for a, (axn, ayn, ar) in items:
        for b, (bxn, byn, br) in items:
            if a >= b:
                continue
            delta = math.hypot((axn - bxn) * width, (ayn - byn) * height)
            separation = delta - (ar + br) * height
            assert separation >= 10, (
                f"{width}x{height}: {a} and {b} too close: {separation:.1f}px"
            )
    print(f"PASS {width}x{height}: GUI scale {scale}, clear buttons and safe margins")

print("PASS Java render circles match Rust touch hitboxes")
