#!/usr/bin/env python3
"""Horizon mobile HUD contract: actual native hotbar, input, and Android draw layers.

Parses the implementations to prevent visual/input coordinate drift. Simulates
desktop, tablet and phone aspect ratios at each control scaling preset. This is
a layout test and is not a substitute for hardware multitouch validation.
"""
from __future__ import annotations
import itertools
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
read = lambda p: (ROOT / p).read_text(encoding="utf-8")
JAVA=read("packaging/android/java/dev/horizon/android/HorizonActivity.java")
PREVIEW=read("crates/horizon-client/src/game/touch_preview.rs")
INPUT=read("crates/horizon-winit/src/lib.rs")
GESTURE=read("crates/horizon-winit/src/touch_gestures.rs")
HOTBAR=read("crates/horizon-client/src/game/touch_hotbar.rs")
# The native Skia Android overlay and Windows Rust preview MUST share exactly
# the same 11 pixel icons. Geometry tests alone cannot catch icon drift.
import re
def icon_masks(code):
    # Both files define exactly 11 rows of 24 pixel-mask values. Never
    # depend on Java/Rust bracket syntax when reading this contract.
    pos=code.index("PIXEL_GLYPHS")
    masks=re.findall(r"0x[0-9a-fA-F]{6}", code[pos:])
    return masks[:11*24]
java_masks=icon_masks(JAVA)
rust_masks=icon_masks(PREVIEW)
assert len(java_masks)==len(rust_masks)==264, "Expected 11 24x24 glyphs"
assert [m.lower() for m in java_masks]==[m.lower() for m in rust_masks], "Android/Rust pixel glyphs have diverged"
# Distinctly recognisable Bedrock crouch: downward arrow above a floor bar.
assert int(java_masks[4*24+20],16)!=0 and int(java_masks[4*24+21],16)!=0

NATIVE=read("crates/horizon-client/src/android_main.rs")
PAD=read("crates/horizon-client/src/game/pad.rs")
GAME=read("crates/horizon-client/src/game.rs")
for code,expected in (
    (HOTBAR,("pub(super) const MAGNIFICATION: f32 = 1.30;",
             "pub(super) const BOTTOM_GAP: f32 = 18.0;",
             "pub(super) const INVENTORY_CAP: f32 = 27.0;",
             "transform_hotbar", "inventory_cap")),
    (GESTURE,("pub const HOTBAR_SCALE: f32 = 1.30;",
              "pub const HOTBAR_BOTTOM: f32 = 18.0;",
              "pub const INVENTORY_CAP: f32 = 27.0;",
              "pub fn hotbar_inventory", "pub fn hotbar_slot")),
    (GAME,("touch_hotbar::transform_hotbar","touch_hotbar::inventory_cap",
           "touch_hotbar::lift_stats")),
    (JAVA,('control(canvas, .5f - .045f / aspect, .045f, .028f, "chat")',
           'control(canvas, .5f + .045f / aspect, .045f, .028f, "pause")',
           'nativeSetControls(scheme, controlScale, alwaysSprint)',
           'sprint.setText("Always sprint when moving forward")')),
    (PREVIEW,("Symbol::Chat", "Symbol::Sprint", "HORIZON_TOUCH_PREVIEW_ALWAYS_SPRINT")),
    (INPUT,("TouchRole::Chat", "TouchRole::SprintToggle",
            "touch_gestures::hotbar_inventory", "TOUCH_ALWAYS_SPRINT",
            "sync_sprint(&mut touch)")),
    (NATIVE,("always_sprint: u8", "always_sprint != 0")),
    (PAD,('winit::AndroidGesture::Chat', 'self.open_chat_method("")')),
):
    for text in expected:
        assert text in code, f"Required implementation missing: {text}"

assert 'control(canvas, inventoryX' not in JAVA, "Floating inventory button resurfaced"
assert "TouchRole::Button(3) // Native hotbar inventory" in INPUT
assert "android_dpad_geometry" in INPUT and "dpadGeometry()" in JAVA
assert "centre sneak key" in INPUT
assert "((x-left)/(20.0*s))" in GESTURE.replace(" ",""), "Adaptive hotbar hit geometry changed"

def box(x,y,r):
    return (x-r,y-r,x+r,y+r)

def distance(a,b):
    return math.hypot(max(a[0]-b[2],b[0]-a[2],0),max(a[1]-b[3],b[1]-a[3],0))

def layout(w,h,scheme,scale):
    gui=1
    while w//(gui+1)>=320 and h//(gui+1)>=240: gui+=1
    gw,gh=math.ceil(w/gui),math.ceil(h/gui)
    mag=min(1.30,(gw/2-10.0)/(91+27))
    rect_bar=(
        (gw/2-91*mag)*gui,
        (gh-18-22*mag)*gui,
        (gw/2+(91+27)*mag)*gui,
        (gh-18)*gui,
    )
    cap=(
        (gw/2+91*mag-1)*gui,
        (gh-18-22*mag)*gui,
        (gw/2+91*mag-1+27*mag)*gui,
        (gh-18)*gui,
    )
    controls={
        "jump":box(w-.160*h,.72*h,.051*h*scale),
        "pause":box(.5*w+.045*h,.045*h,.028*h*scale),
        "chat":box(.5*w-.045*h,.045*h,.028*h*scale),
    }
    if scheme & 1:
        controls.update({
            "attack":box(w-.270*h,.56*h,.042*h*scale),
            "use":box(w-.145*h,.52*h,.042*h*scale),
        })
    if scheme < 2:
        controls.update({
            "sneak":box(w-.290*h,.73*h,.040*h*scale),
            "sprint":box(.11*w+.190*h,.57*h,.035*h*scale),
            "joystick":box(.11*w,.705*h,.092*h*scale),
        })
    else:
        off=.147*h*scale
        r=.068*h*scale
        cx=max(.11*w,.23*h*scale)
        cy=.72*h
        if cx+off+r > rect_bar[0]-9:
            cy=min(cy,rect_bar[1]-9-r)
        if cx+r > rect_bar[0]-9:
            cy=min(cy,rect_bar[1]-9-off-r)
        controls["sprint"]=box(cx+1.55*off,cy-1.45*off,.035*h*scale)
        controls["sneak"]=box(cx,cy,r)
        for name,dx,dy in [("up",0,-1),("right",1,0),("down",0,1),("left",-1,0)]:
            controls["dpad_"+name]=box(cx+dx*off,cy+dy*off,r)
    return controls,rect_bar,cap

screens=((800,600),(1280,720),(1280,800),(1600,900),(1600,1200),
         (1920,1080),(2400,1080),(2560,1080),(3200,1800))
count=0
for (w,h),scale,scheme in itertools.product(screens,(.8,1.0,1.2),range(4)):
    controls,bar,cap=layout(w,h,scheme,scale)
    for name,area in controls.items():
        a,b,c,d=area
        assert min(a,b,w-c,h-d) >= 6, (w,h,scheme,scale,name,"off screen",area)
        assert distance(area,bar) >= 6, (w,h,scheme,scale,name,"hotbar collision",area,bar)
    for (na,a),(nb,b) in itertools.combinations(controls.items(),2):
        if na.startswith("dpad_") and nb.startswith("dpad_"): continue
        assert distance(a,b)>=5, (w,h,scheme,scale,na,nb,"overlap",a,b)
    assert bar[0]>=8 and bar[2]<=w-8 and bar[3]<h-8, (w,h,bar)
    count+=1
print(f"PASS: {count} device/mode/scale combinations, true hotbar end-cap and touch geometry")
print("PASS: no action overlaps the raised hotbar or another button")
print("PASS: sprint/always-sprint/chat JNI, Android input, Windows preview, native HUD routing")
