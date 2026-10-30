//! Clean-room Bedrock-style Android touch HUD primitives.
//!
//! The shapes deliberately use Horizon's white HUD texel instead of Mojang
//! artwork.  We mirror the mobile visual language (translucent rounded plates,
//! white line glyphs, joystick/D-pad affordances and pressed states) while
//! keeping all pixels original to Horizon.

use horizon_client::hud::{HudAtlas, HudBatch, rgba};

#[derive(Clone, Copy, Debug)]
pub(super) enum Icon {
    Jump,
    Sneak,
    Sprint,
    Attack,
    Use,
    Inventory,
    Chat,
    Pause,
    Perspective,
}

fn tri(batch: &mut HudBatch, atlas: &HudAtlas, a: [f32; 2], b: [f32; 2], c: [f32; 2], color: u32) {
    batch.fill_quad(atlas, [a, b, c, a], color);
}

fn segment(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    a: [f32; 2],
    b: [f32; 2],
    width: f32,
    color: u32,
) {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let nx = -dy / len * width * 0.5;
    let ny = dx / len * width * 0.5;
    batch.fill_quad(
        atlas,
        [
            [a[0] + nx, a[1] + ny],
            [b[0] + nx, b[1] + ny],
            [b[0] - nx, b[1] - ny],
            [a[0] - nx, a[1] - ny],
        ],
        color,
    );
}

/// An eight-point rounded-square approximation.  The old HUD used regular
/// octagons, which read as generic gamepad buttons rather than Minecraft
/// Bedrock's squarer mobile plates.
fn plate(batch: &mut HudBatch, atlas: &HudAtlas, cx: f32, cy: f32, r: f32, color: u32) {
    let c = r * 0.24;
    let points = [
        [cx - r + c, cy - r],
        [cx + r - c, cy - r],
        [cx + r, cy - r + c],
        [cx + r, cy + r - c],
        [cx + r - c, cy + r],
        [cx - r + c, cy + r],
        [cx - r, cy + r - c],
        [cx - r, cy - r + c],
    ];
    for i in 0..8 {
        tri(batch, atlas, [cx, cy], points[i], points[(i + 1) & 7], color);
    }
}

fn ring_plate(batch: &mut HudBatch, atlas: &HudAtlas, cx: f32, cy: f32, r: f32, down: bool) {
    plate(
        batch,
        atlas,
        cx,
        cy,
        r,
        rgba(255, 255, 255, if down { 122 } else { 58 }),
    );
    plate(
        batch,
        atlas,
        cx,
        cy,
        (r - 1.4).max(1.0),
        rgba(18, 20, 23, if down { 148 } else { 94 }),
    );
}

pub(super) fn joystick(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    cx: f32,
    cy: f32,
    r: f32,
    vector: [f32; 2],
) {
    // Bedrock's newer joystick is a quiet translucent base with a brighter
    // thumb cap.  A rounded-square ring also survives tiny GUI scales better
    // than a true circle rendered from quads.
    plate(batch, atlas, cx, cy, r, rgba(255, 255, 255, 24));
    plate(batch, atlas, cx, cy, r * 0.92, rgba(17, 19, 22, 50));

    let knob = r * 0.37;
    let kx = cx + vector[0] * r * 0.78;
    let ky = cy - vector[1] * r * 0.78;
    let moving = vector != [0.0, 0.0];
    plate(
        batch,
        atlas,
        kx,
        ky,
        knob,
        rgba(255, 255, 255, if moving { 122 } else { 64 }),
    );
    plate(
        batch,
        atlas,
        kx,
        ky,
        (knob - 1.5).max(1.0),
        rgba(38, 40, 44, if moving { 104 } else { 72 }),
    );
}

/// Bedrock classic D-pad.  `vector` is the active digital direction and may be
/// diagonal.  Each arm lights independently so sliding a thumb through the pad
/// has immediate feedback.
pub(super) fn dpad(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    cx: f32,
    cy: f32,
    r: f32,
    vector: [f32; 2],
) {
    let arm = r * 0.46;
    let gap = r * 0.10;
    let positions = [
        ([0.0, -1.0], cx, cy - r * 0.62),
        ([0.0, 1.0], cx, cy + r * 0.62),
        ([-1.0, 0.0], cx - r * 0.62, cy),
        ([1.0, 0.0], cx + r * 0.62, cy),
    ];
    for (dir, x, y) in positions {
        let down = (dir[0] != 0.0 && vector[0] == dir[0])
            || (dir[1] != 0.0 && -vector[1] == dir[1]);
        ring_plate(batch, atlas, x, y, arm, down);
        let color = rgba(255, 255, 255, if down { 255 } else { 210 });
        let tip = arm * 0.55;
        let tail = arm * 0.16;
        if dir[1] < 0.0 {
            tri(batch, atlas, [x, y - tip], [x - tip * 0.62, y + tail], [x + tip * 0.62, y + tail], color);
        } else if dir[1] > 0.0 {
            tri(batch, atlas, [x, y + tip], [x - tip * 0.62, y - tail], [x + tip * 0.62, y - tail], color);
        } else if dir[0] < 0.0 {
            tri(batch, atlas, [x - tip, y], [x + tail, y - tip * 0.62], [x + tail, y + tip * 0.62], color);
        } else {
            tri(batch, atlas, [x + tip, y], [x - tail, y - tip * 0.62], [x - tail, y + tip * 0.62], color);
        }
    }
    plate(batch, atlas, cx, cy, gap, rgba(255, 255, 255, 32));
}

pub(super) fn button(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    icon: Icon,
    cx: f32,
    cy: f32,
    r: f32,
    down: bool,
) {
    ring_plate(batch, atlas, cx, cy, r, down);
    draw_icon(
        batch,
        atlas,
        icon,
        cx,
        cy,
        r * 0.58,
        rgba(255, 255, 255, if down { 255 } else { 224 }),
    );
}

fn draw_icon(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    icon: Icon,
    cx: f32,
    cy: f32,
    s: f32,
    color: u32,
) {
    match icon {
        Icon::Jump => {
            let stem = s * 0.17;
            batch.fill(atlas, cx - stem * 0.5, cy - s * 0.02, stem, s * 0.66, color);
            tri(batch, atlas, [cx, cy - s * 0.72], [cx - s * 0.55, cy - s * 0.08], [cx + s * 0.55, cy - s * 0.08], color);
        }
        Icon::Sneak => {
            segment(batch, atlas, [cx - s * 0.56, cy - s * 0.30], [cx, cy + s * 0.22], s * 0.16, color);
            segment(batch, atlas, [cx + s * 0.56, cy - s * 0.30], [cx, cy + s * 0.22], s * 0.16, color);
            segment(batch, atlas, [cx - s * 0.58, cy + s * 0.52], [cx + s * 0.58, cy + s * 0.52], s * 0.12, color);
        }
        Icon::Sprint => {
            // Bedrock-like double forward chevron.
            for oy in [-s * 0.26, s * 0.27] {
                segment(batch, atlas, [cx - s * 0.48, cy + oy + s * 0.18], [cx, cy + oy - s * 0.20], s * 0.14, color);
                segment(batch, atlas, [cx + s * 0.48, cy + oy + s * 0.18], [cx, cy + oy - s * 0.20], s * 0.14, color);
            }
        }
        Icon::Attack => {
            segment(batch, atlas, [cx - s * 0.42, cy + s * 0.42], [cx + s * 0.42, cy - s * 0.42], s * 0.18, color);
            tri(batch, atlas, [cx + s * 0.66, cy - s * 0.66], [cx + s * 0.30, cy - s * 0.47], [cx + s * 0.47, cy - s * 0.30], color);
            segment(batch, atlas, [cx - s * 0.56, cy + s * 0.12], [cx - s * 0.12, cy + s * 0.56], s * 0.14, color);
        }
        Icon::Use => {
            // Open hand/pointer: distinct from attack at phone scale.
            let w = s * 0.17;
            batch.fill(atlas, cx - s * 0.29, cy - s * 0.04, s * 0.70, s * 0.54, color);
            batch.fill(atlas, cx - s * 0.26, cy - s * 0.60, w, s * 0.62, color);
            batch.fill(atlas, cx + s * 0.01, cy - s * 0.38, w, s * 0.42, color);
            batch.fill(atlas, cx + s * 0.28, cy - s * 0.26, w, s * 0.32, color);
            segment(batch, atlas, [cx - s * 0.34, cy + s * 0.32], [cx - s * 0.58, cy + s * 0.08], s * 0.17, color);
        }
        Icon::Inventory => {
            let cell = s * 0.29;
            let gap = s * 0.10;
            let total = cell * 3.0 + gap * 2.0;
            let x0 = cx - total * 0.5;
            let y0 = cy - total * 0.5;
            for row in 0..3 {
                for col in 0..3 {
                    batch.fill(atlas, x0 + col as f32 * (cell + gap), y0 + row as f32 * (cell + gap), cell, cell, color);
                }
            }
        }
        Icon::Chat => {
            let w = s * 1.23;
            let h = s * 0.80;
            let x = cx - w * 0.5;
            let y = cy - h * 0.55;
            let line = (s * 0.13).max(1.1);
            batch.fill(atlas, x, y, w, line, color);
            batch.fill(atlas, x, y + h - line, w, line, color);
            batch.fill(atlas, x, y, line, h, color);
            batch.fill(atlas, x + w - line, y, line, h, color);
            tri(batch, atlas, [cx - s * 0.28, y + h - line], [cx - s * 0.52, cy + s * 0.64], [cx + s * 0.02, y + h - line], color);
        }
        Icon::Pause => {
            let w = s * 0.27;
            batch.fill(atlas, cx - s * 0.47, cy - s * 0.61, w, s * 1.22, color);
            batch.fill(atlas, cx + s * 0.20, cy - s * 0.61, w, s * 1.22, color);
        }
        Icon::Perspective => {
            // Camera/perspective eye.  It avoids Mojang's sprite while keeping
            // the Bedrock top-HUD affordance immediately recognisable.
            tri(batch, atlas, [cx - s * 0.68, cy], [cx, cy - s * 0.46], [cx, cy + s * 0.46], color);
            tri(batch, atlas, [cx + s * 0.68, cy], [cx, cy - s * 0.46], [cx, cy + s * 0.46], color);
            plate(batch, atlas, cx, cy, s * 0.18, rgba(18, 20, 23, 230));
            plate(batch, atlas, cx, cy, s * 0.09, color);
        }
    }
}
