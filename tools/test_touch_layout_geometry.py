"""Regression guard for the same touch layout in Android Canvas, winit hit tests,
and Horizon's native Windows ARM64 debug renderer.

No external packages required: python tools/test_touch_layout_geometry.py
"""
from __future__ import annotations
from pathlib import Path
from math import hypot, pi

ROOT = Path(__file__).resolve().parents[1]
JAVA = (ROOT / "packaging/android/java/dev/horizon/android/HorizonActivity.java").read_text()
PREVIEW = (ROOT / "crates/horizon-client/src/game/touch_preview.rs").read_text()
INPUT = (ROOT / "crates/horizon-winit/src/lib.rs").read_text()

# Assert the three implementations retain the same positions and visual radii.
SYMBOLS = [
    ("jump", "1f - .160f / aspect, .77f, .051f",
     "w - 0.160 * h, 0.77 * h, 0.051 * h",
     "[1.0 - 0.160 / aspect, 0.77], 0.051 * scale"),
    ("sneak", "1f - .290f / aspect, .77f, .040f",
     "w - 0.290 * h, 0.77 * h, 0.040 * h",
     "[1.0 - 0.290 / aspect, 0.77], 0.040 * scale"),
    ("hit", "1f - .270f / aspect, .56f, .042f",
     "w - 0.270 * h, 0.56 * h, 0.042 * h",
     "[1.0 - 0.270 / aspect, 0.56], 0.042 * scale"),
    ("use", "1f - .145f / aspect, .52f, .042f",
     "w - 0.145 * h, 0.52 * h, 0.042 * h",
     "[1.0 - 0.145 / aspect, 0.52], 0.042 * scale"),
    ("sprint", ".11f + .190f / aspect, .62f, .035f",
     "0.11 * w + 0.190 * h, 0.62 * h, 0.035 * h",
     "[0.11 + 0.190 / aspect, 0.62], 0.035 * scale"),
    ("pause", ".50f, .072f, .032f",
     "0.50 * w, 0.072 * h, 0.032 * h",
     "[0.50, 0.072], 0.032 * scale"),
]
for name, java_expr, rust_expr, input_expr in SYMBOLS:
    assert "".join(java_expr.split()) in "".join(JAVA.split()), f"Java control layout diverged: {name}"
    assert "".join(rust_expr.split()) in "".join(PREVIEW.split()), f"Windows preview diverged: {name}"
    assert "".join(input_expr.split()) in "".join(INPUT.split()), f"Android input hit area diverged: {name}"

for source, value in (
    (JAVA, "Math.max(.11f * getWidth(), .205f * getHeight())"),
    (PREVIEW, "(0.11 * w).max(0.205 * h)"),
    (INPUT, "0.11_f64.max(0.205 / aspect)"),
):
    assert value in source, "D-pad tablet inset diverged from shared geometry"

for source, snippets in (
    (JAVA, ("float inventoryX = Math.max(.735f * getWidth()", "inventoryX / getWidth(), .92f, .030f")),
    (PREVIEW, ("let inventory_x = (0.735 * w).max(", "draw.control(inventory_x, 0.92 * h")),
    (INPUT, ("let inventory_x = (0.735 * width).max(", "[inventory_x / width, 0.92], 0.030 * scale")),
):
    normalized = "".join(source.split())
    for snippet in snippets:
        assert "".join(snippet.split()) in normalized, "Dynamic inventory control diverged"

def layout(aspect: float, scheme: int, scale: float) -> dict[str, tuple[float, float, float]]:
    w = aspect
    items = {}
    if scheme < 2:
        items["joystick"] = (.11*w, .745, .092*scale)
    else:
        cx = max(.11*w, .205)
        for name, dx, dy in (
            ("up",0,-1), ("right",1,0), ("down",0,1), ("left",-1,0)
        ):
            items[name] = (cx + dx*.068*1.48*scale, .745+dy*.068*1.48*scale, .043*scale)
    h = 1080.0
    gui_scale = max(1, min(int(w * h // 320), int(h // 240)))
    inventory_x = max(.735*w, .5*w + 91*gui_scale/h + .030*scale + max(14/h,.015))
    items.update(
        jump=(w-.160,.77,.051*scale),
        sneak=(w-.290,.77,.040*scale),
        sprint=(.11*w+.190,.62,.035*scale),
        inventory=(inventory_x,.92,.030*scale),
        pause=(.50*w,.072,.032*scale)
    )
    if scheme & 1:
        items["hit"] = (w-.270,.56,.042*scale)
        items["use"] = (w-.145,.52,.042*scale)
    return items

old = {
    "joystick": pi*.135**2,
    "jump": 4*.085**2,
    "sneak": 4*.065**2,
    "hit": 4*.073**2,
    "use": 4*.070**2,
    "sprint": 4*.055**2,
    "inventory": 4*.055**2,
    "pause": 4*.055**2,
}
checks = 0
for aspect in (1.30, 4/3, 1.50, 16/9, 2.0, 20/9, 2.40):
    for scale in (.8,1.0,1.2):
        for scheme in range(4):
            items=layout(aspect,scheme,scale)
            for name,(x,y,r) in items.items():
                assert x-r>0 and y-r>0 and x+r<aspect and y+r<1, (
                    "screen edge",aspect,scale,scheme,name,(x,y,r))
                # Middle 40% of screen must remain available for looking.
                if name not in ("pause","inventory"):
                    assert not (0.30*aspect < x < .70*aspect and .16 < y < .85), (
                        "button obstructs aiming",name,aspect,scale)
            # Inventory must clear the right edge of the native Java hotbar.
            h = 1080.0
            gui_scale = max(1, min(int(aspect * h // 320), int(h // 240)))
            inv_x,_,inv_r=items["inventory"]
            assert (inv_x - inv_r) * h > (.5 * aspect * h + 91 * gui_scale + 12), (
                "inventory covers hotbar",aspect,scheme,scale
            )
            names=list(items)
            for i,name in enumerate(names):
                x,y,r=items[name]
                for other in names[i+1:]:
                    u,v,s=items[other]
                    assert hypot(x-u,y-v)+1e-5 >= r+s, (
                        "visual overlap",aspect,scale,scheme,name,other)
            checks+=1
print(f"PASS: {checks} mode/scale/aspect layouts; Android, Rust hit testing and GPU preview agree")
print("PASS: central aim region clear; all controls inside visible screen; no visual collisions")
old_mode1=sum(old.values())
new_mode1=sum((pi if k=="joystick" else 4)*r*r for k,(x,y,r) in layout(16/9,1,1).items())
print(f"PASS: control artwork footprint down by {(1-new_mode1/old_mode1):.1%} in crosshair mode")
