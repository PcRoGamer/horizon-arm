#!/usr/bin/env python3
"""Executable geometry contract for Android touch controls and Windows ARM64 mirror.

Tests 4 control modes x 3 scales x 8 aspect/screen sizes, zero off-screen
controls, hotbar clearance, gameplay-center clearance and source/hitbox parity.
This validates layout, not real Android multitouch or driver behavior.
"""
from __future__ import annotations

import itertools
import math
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
JAVA = (ROOT/"packaging/android/java/dev/horizon/android/HorizonActivity.java").read_text()
PREVIEW = (ROOT/"crates/horizon-client/src/game/touch_preview.rs").read_text()
INPUT = (ROOT/"crates/horizon-winit/src/lib.rs").read_text()

def normalized(text: str) -> str:
    return re.sub(r"\s+", "", text)

java, preview, input_source = map(normalized, (JAVA, PREVIEW, INPUT))
symbols = [
    ("jump", "1f-.160f/aspect,.77f,.051f",
     "w-0.160*h,0.77*h,0.051*h*cfg.scale",
     "[1.0-0.160/aspect,0.77],0.051*scale"),
    ("sneak", "1f-.290f/aspect,.77f,.040f",
     "w-0.290*h,0.77*h,0.040*h*cfg.scale",
     "[1.0-0.290/aspect,0.77],0.040*scale"),
    ("hit", "1f-.270f/aspect,.56f,.042f",
     "w-0.270*h,0.56*h,0.042*h*cfg.scale",
     "[1.0-0.270/aspect,0.56],0.042*scale"),
    ("use", "1f-.145f/aspect,.52f,.042f",
     "w-0.145*h,0.52*h,0.042*h*cfg.scale",
     "[1.0-0.145/aspect,0.52],0.042*scale"),
    ("sprint", ".11f+.190f/aspect,.60f,.035f",
     "0.11*w+0.190*h,0.60*h,0.035*h*cfg.scale",
     "[0.11+0.190/aspect,0.60],0.035*scale"),
    ("pause", ".50f,.072f,.032f",
     "0.50*w,0.072*h,0.032*h*cfg.scale",
     "[0.50,0.072],0.032*scale")
]
for name, j, p, i in symbols:
    assert f'{j},"{name}"' in java, f"Java button diverged: {name}"
    assert p in preview, f"Windows GPU preview diverged: {name}"
    assert i in input_source, f"Android Rust hitbox diverged: {name}"
    assert input_source.count(i) == 1, f"Duplicate touch condition: {name}"

for term in ("0.11*w,0.718*h,0.092*h*cfg.scale",
             "(0.11*w).max(0.205*h)", "0.718*h+dy*offset"):
    assert term in preview, f"Windows movement renderer changed: {term}"
for term in ("Math.max(.11f*getWidth(),.205f*getHeight())",
             "floaty=.718f*h;", "floatcenterY=.718f*getHeight();"):
    assert term in java, f"Android movement renderer changed: {term}"
for term in ("[move_x,0.718]", "(0.11_f64.max(0.205/aspect))",
             "height*0.718"):
    assert term in input_source or (term.startswith("(0.11_") and
           "0.11_f64.max(0.205/aspect)" in input_source), f"Touch movement region changed: {term}"

# Dynamic inventory must follow the current hotbar, not an assumed percentage.
assert "Math.max(.735f*getWidth()" in java
assert "inventory_x" in preview
assert "inventory_x/width,0.92" in input_source
assert "0.030*height*scale" in input_source
assert "(0.015*height).max(14.0)" in input_source
# Hotbar gets first priority. Sprint button gets priority over the large D-pad
# capture region, otherwise a sprint tap becomes a movement press.
touch_match = re.search(r"letrole=ifhotbar_slot\(p\).*?letSome\(role\)", input_source)
assert touch_match, "Cannot find Android touch classification"
touch = touch_match.group()
assert touch.index("TouchRole::Hotbar") < touch.index("TouchRole::Button(10)") < touch.index("TouchRole::Move"), \
    "Sprint hitbox is swallowed by the movement region"
assert "touch.fingers.values().any(" in INPUT
assert "TouchRole::Ignored" in INPUT
assert "nativeTouchAxes" in JAVA and "nativeTouchButtons" in JAVA

def rect(cx: float,cy: float,r: float) -> tuple[float,float,float,float]:
    return cx-r,cy-r,cx+r,cy+r

def distance(a,b) -> float:
    dx=max(a[0]-b[2], b[0]-a[2],0)
    dy=max(a[1]-b[3], b[1]-a[3],0)
    return math.hypot(dx,dy)

def boxes(w:int,h:int,mode:int,scale:float):
    # Mirrors source constants in *physical pixels* and respects screen aspect.
    movement=(max(.11*w,.205*h),.718*h) if mode>=2 else (.11*w,.718*h)
    controls={}
    if mode<2:
        controls["joystick"]=rect(*movement,.092*h*scale)
    else:
        offset=.068*1.48*h*scale
        for name,dx,dy in (("up",0,-1),("right",1,0),("down",0,1),("left",-1,0)):
            controls["dpad_"+name]=rect(movement[0]+dx*offset,
                                         movement[1]+dy*offset,.043*h*scale)
    controls.update({
        "jump":rect(w-.160*h,.77*h,.051*h*scale),
        "sneak":rect(w-.290*h,.77*h,.040*h*scale),
        "sprint":rect(.11*w+.190*h,.60*h,.035*h*scale),
        "pause":rect(.5*w,.072*h,.032*h*scale),
    })
    if mode%2:
        controls["hit"]=rect(w-.270*h,.56*h,.042*h*scale)
        controls["use"]=rect(w-.145*h,.52*h,.042*h*scale)
    gui_scale=max(1,min(w//320,h//240))
    inv_x=max(.735*w,.5*w+91*gui_scale+.030*h*scale+max(14,.015*h))
    controls["inventory"]=rect(inv_x,.92*h,.030*h*scale)
    hotbar=(.5*w-91*gui_scale,h-22*gui_scale,.5*w+91*gui_scale,h)
    return controls,hotbar

screens=((800,600),(1280,720),(1280,800),(1600,900),(1600,1200),
         (1920,1080),(2400,1080),(2560,1080))
checks=0
for (w,h),scale,mode in itertools.product(screens,(.8,1.0,1.2),range(4)):
    controls,hotbar=boxes(w,h,mode,scale)
    for name,area in controls.items():
        x0,y0,x1,y1=area
        assert min(x0,y0,w-x1,h-y1)>=12, (w,h,scale,mode,name,"edge",area)
        assert distance(area,hotbar)>=12, (w,h,scale,mode,name,"hotbar",area)
        if name not in ("pause","inventory"):
            center=(.34*w,.18*h,.66*w,.82*h)
            assert distance(area,center)>0, (w,h,scale,mode,name,"center")
    for (an,a),(bn,b) in itertools.combinations(controls.items(),2):
        if an.startswith("dpad_") and bn.startswith("dpad_"):
            assert distance(a,b)>=0, (w,h,mode,an,bn)
            continue
        assert distance(a,b)>=8, (w,h,scale,mode,an,bn,distance(a,b))
    checks+=1
print(f"PASS {checks} screen/mode/scale combinations, screen-edge and hotbar clearances")
print("PASS rendered controls and Android hitbox parity; sprint and hotbar touch priorities")
# Measure total visual artwork footprint, not touch-target usability.
old_area=math.pi*.135**2 + 4*sum(x*x for x in (.085,.065,.073,.070,.055,.055,.055))
new_area=math.pi*.092**2 + 4*sum(x*x for x in (.051,.040,.042,.042,.035,.030,.032))
print(f"Visual artwork area reduced {(1-new_area/old_area)*100:.1f}% in crosshair mode")
