#!/usr/bin/env python3
"""Render actual Horizon hotbar assets and Rust touch_hotbar geometry, no game startup.

This renders the *real* Minecraft sprites, not recreated slot rectangles.
The compositing rasterizer is Pillow, not production WGPU: visual preview only.
"""
from __future__ import annotations
import argparse
import math
import re
from pathlib import Path

try:
    from PIL import Image
except ImportError as exc:
    raise SystemExit("Pillow required: python -m pip install Pillow") from exc

ROOT = Path(__file__).resolve().parents[1]
RUST = ROOT / "crates/horizon-client/src/game/touch_hotbar.rs"


def constants():
    source = RUST.read_text(encoding="utf-8")
    assert "fn transform_hotbar" in source and "fn inventory_cap" in source
    config = {}
    for key in ("MAGNIFICATION", "BOTTOM_GAP", "INVENTORY_CAP"):
        match = re.search(rf"const\s+{key}:\s*f32\s*=\s*([0-9.]+)\s*;", source)
        if not match:
            raise ValueError("Production Rust HUD constant not found: " + key)
        config[key] = float(match.group(1))
    return config


def gui_scale(width, height):
    scale = 1
    while width // (scale + 1) >= 320 and height // (scale + 1) >= 240:
        scale += 1
    return scale


def blend_rect(img, x, y, width, height, color):
    l, t, r, b = map(round, (x, y, x + width, y + height))
    if r > l and b > t:
        img.alpha_composite(Image.new("RGBA", (r-l, b-t), color), (l, t))


def sprite(img, path, x, y, width, height):
    image = Image.open(path).convert("RGBA")
    img.alpha_composite(
        image.resize((round(width), round(height)), Image.Resampling.NEAREST),
        (round(x), round(y)),
    )


def render(resources, width, height, selected=0):
    if width < 320 or height < 240 or selected not in range(9):
        raise ValueError("Invalid viewport or slot")
    config = constants()
    bottom = config["BOTTOM_GAP"]
    cap_width = config["INVENTORY_CAP"]
    gui = gui_scale(width, height)
    gw, gh = math.ceil(width / gui), math.ceil(height / gui)
    mag = min(config["MAGNIFICATION"], (gw * .5 - 10.0) / (91.0 + cap_width))
    cx = gw * .5
    center = gw // 2
    base = gh - bottom
    hud = (resources / "assets/minecraft/textures/gui/sprites/hud")
    bar_path = hud / "hotbar.png"
    sel_path = hud / "hotbar_selection.png"
    if Image.open(bar_path).size != (182, 22):
        raise ValueError("Unexpected production hotbar texture dimensions")
    if Image.open(sel_path).size != (24, 23):
        raise ValueError("Unexpected production selection dimensions")
    canvas = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    tx = lambda x: (cx + (x - cx) * mag) * gui
    ty = lambda y: (base + (y - gh) * mag) * gui
    # hud_layout::hotbar sprite calls, transformed in touch_hotbar.rs.
    sprite(canvas, bar_path, tx(center-91), ty(gh-22), 182*mag*gui, 22*mag*gui)
    sprite(canvas, sel_path, tx(center-92+20*selected), ty(gh-23),
           24*mag*gui, 23*mag*gui)

    # The exact source-controlled inventory endcap geometry and fill colours.
    side = 22*mag
    cap = cap_width*mag
    x = (gw*.5 + 91*mag - 1)*gui
    y = (gh-bottom-side)*gui
    p = lambda a: a*gui
    blend_rect(canvas, x, y, p(cap), p(side), (11,15,16,231))
    blend_rect(canvas, x+p(1.5*mag), y+p(1.5*mag),
               p(cap-3*mag), p(side-3*mag), (57,63,64,203))
    blend_rect(canvas, x+p(2.5*mag), y+p(2.5*mag),
               p(cap-5*mag), p(side-5*mag), (102,108,106,119))
    blend_rect(canvas, x+p(2.5*mag), y+p(side-3.3*mag),
               p(cap-5*mag), p(.8*mag), (29,34,36,226))
    dot, gap = 3.5*mag, 2.5*mag
    dx = x+p((cap-(3*dot+2*gap))*.5)
    dy = y+p((side-dot)*.5)
    for n in range(3):
        blend_rect(canvas, dx+p(n*(dot+gap)), dy, p(dot), p(dot),
                   (219,227,224,235))
    return canvas


def render_survival_icons(canvas, resources, width, height):
    """Actual Minecraft heart/food textures at production health anchor."""
    gui = gui_scale(width, height)
    gw = math.ceil(width / gui)
    sprite_root = resources / "assets/minecraft/textures/gui/sprites/hud"
    for i in range(10):
        # HealthHud::layout emits 9px sprites at x_left+n*8 and
        # x_right-n*8-9; anchor_health_to_top scales real vertices 1.25.
        left = (4.0 + i * 8.0 * 1.25) * gui
        right = (gw - 4.0 - (9.0 + i * 8.0) * 1.25) * gui
        y = 3.0 * gui
        for path, x in (("heart/container", left), ("heart/full", left),
                        ("food_empty", right), ("food_full", right)):
            sprite(canvas, sprite_root / (path + ".png"), x, y,
                   9.0 * 1.25 * gui, 9.0 * 1.25 * gui)


def render_experience(canvas, resources, width, height, progress):
    """Production atlas textures and touch_hotbar::transform_experience."""
    if not 0 <= progress <= 1:
        raise ValueError("XP progress must be in [0,1]")
    gui = gui_scale(width, height)
    gw, gh = math.ceil(width / gui), math.ceil(height / gui)
    cfg = constants()
    s = min(cfg["MAGNIFICATION"],
            (gw * .5 - 10.0) / (91.0 + cfg["INVENTORY_CAP"]))
    x = (gw * .5 - 91.0 * s) * gui
    y = (gh - cfg["BOTTOM_GAP"] - 22.0 * s - 9.0) * gui
    dest_w, dest_h = 182.0 * s * gui, 5.0 * s * gui
    root = resources / "assets/minecraft/textures/gui/sprites/hud"
    sprite(canvas, root / "experience_bar_background.png",
           x, y, dest_w, dest_h)
    if progress > 0:
        bar = Image.open(root / "experience_bar_progress.png").convert("RGBA")
        portion = max(1, round(bar.width * progress))
        fill = bar.crop((0, 0, portion, bar.height))
        canvas.alpha_composite(fill.resize(
            (round(dest_w * progress), round(dest_h)),
            Image.Resampling.NEAREST), (round(x), round(y)))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--resources", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--size", default="1200x630", help="Physical pixels, e.g. 1200x630")
    parser.add_argument("--slot", type=int, default=0)
    parser.add_argument("--survival", action="store_true",
                        help="Preview healthy survival hearts/hunger from actual sprite files")
    parser.add_argument("--xp", type=float, default=None,
                        help="Optional experience bar progress 0..1")
    parser.add_argument("--android-png", type=Path,
                        help="Native Skia production Android Canvas capture, same viewport")
    args = parser.parse_args()
    width, height = map(int, args.size.lower().split("x"))
    image = render(args.resources, width, height, args.slot)
    if args.xp is not None:
        render_experience(image, args.resources, width, height, args.xp)
    if args.survival:
        render_survival_icons(image, args.resources, width, height)
    if args.android_png:
        overlay = Image.open(args.android_png).convert("RGBA")
        if overlay.size != image.size:
            raise ValueError("Android and hotbar captures must match exactly")
        image.alpha_composite(overlay)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    image.save(args.out)
    assert image.getbbox() is not None
    print(f"HORIZON_HUD_ASSET_CAPTURE_OK {args.out} {width}x{height}")


if __name__ == "__main__":
    main()
