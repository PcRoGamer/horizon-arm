//! Minimal icon-first Android touch HUD primitives.
//!
//! These glyphs are drawn from Horizon's existing HUD white texel instead of
//! shipping another texture atlas. They stay sharp at every GUI scale and keep
//! the mobile controls visually separate from desktop key labels.

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
}

fn tri(batch: &mut HudBatch, atlas: &HudAtlas, a: [f32; 2], b: [f32; 2], c: [f32; 2], color: u32) {
    // A degenerate fourth point turns HudBatch's quad into one triangle.
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

fn octagon(batch: &mut HudBatch, atlas: &HudAtlas, cx: f32, cy: f32, r: f32, color: u32) {
    let k = 0.414_213_57 * r;
    let points = [
        [cx - k, cy - r],
        [cx + k, cy - r],
        [cx + r, cy - k],
        [cx + r, cy + k],
        [cx + k, cy + r],
        [cx - k, cy + r],
        [cx - r, cy + k],
        [cx - r, cy - k],
    ];
    for i in 0..8 {
        tri(batch, atlas, [cx, cy], points[i], points[(i + 1) & 7], color);
    }
}

pub(super) fn joystick(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    cx: f32,
    cy: f32,
    r: f32,
    vector: [f32; 2],
) {
    octagon(batch, atlas, cx, cy, r, rgba(7, 9, 13, 46));
    octagon(batch, atlas, cx, cy, r * 0.82, rgba(255, 255, 255, 17));

    // Four small directional ticks make the affordance readable without a
    // permanent D-pad. They fade behind the moving thumb knob.
    let tick = rgba(255, 255, 255, 42);
    let d = r * 0.54;
    let q = r * 0.10;
    segment(batch, atlas, [cx, cy - d - q], [cx, cy - d + q], q * 0.65, tick);
    segment(batch, atlas, [cx, cy + d - q], [cx, cy + d + q], q * 0.65, tick);
    segment(batch, atlas, [cx - d - q, cy], [cx - d + q, cy], q * 0.65, tick);
    segment(batch, atlas, [cx + d - q, cy], [cx + d + q, cy], q * 0.65, tick);

    let knob = r * 0.39;
    let kx = cx + vector[0] * r;
    let ky = cy - vector[1] * r;
    let moving = vector != [0.0, 0.0];
    octagon(
        batch,
        atlas,
        kx,
        ky,
        knob,
        rgba(255, 255, 255, if moving { 146 } else { 74 }),
    );
    octagon(batch, atlas, kx, ky, knob * 0.76, rgba(18, 20, 25, 76));
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
    octagon(
        batch,
        atlas,
        cx,
        cy,
        r,
        rgba(7, 9, 13, if down { 150 } else { 68 }),
    );
    octagon(
        batch,
        atlas,
        cx,
        cy,
        r - 2.0,
        if down {
            rgba(255, 255, 255, 78)
        } else {
            rgba(53, 57, 66, 45)
        },
    );
    draw_icon(
        batch,
        atlas,
        icon,
        cx,
        cy,
        r * 0.58,
        rgba(255, 255, 255, if down { 255 } else { 222 }),
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
            let stem = s * 0.18;
            batch.fill(atlas, cx - stem * 0.5, cy - s * 0.05, stem, s * 0.72, color);
            tri(
                batch,
                atlas,
                [cx, cy - s * 0.76],
                [cx - s * 0.58, cy - s * 0.08],
                [cx + s * 0.58, cy - s * 0.08],
                color,
            );
        }
        Icon::Sneak => {
            // A low chevron over a floor line: visually reads as crouch/down.
            segment(
                batch,
                atlas,
                [cx - s * 0.56, cy - s * 0.35],
                [cx, cy + s * 0.18],
                s * 0.17,
                color,
            );
            segment(
                batch,
                atlas,
                [cx + s * 0.56, cy - s * 0.35],
                [cx, cy + s * 0.18],
                s * 0.17,
                color,
            );
            segment(
                batch,
                atlas,
                [cx - s * 0.58, cy + s * 0.54],
                [cx + s * 0.58, cy + s * 0.54],
                s * 0.13,
                color,
            );
        }
        Icon::Sprint => {
            // Two stacked forward chevrons, like a mobile game's boost glyph.
            for oy in [-s * 0.25, s * 0.28] {
                segment(
                    batch,
                    atlas,
                    [cx - s * 0.50, cy + oy + s * 0.20],
                    [cx, cy + oy - s * 0.20],
                    s * 0.15,
                    color,
                );
                segment(
                    batch,
                    atlas,
                    [cx + s * 0.50, cy + oy + s * 0.20],
                    [cx, cy + oy - s * 0.20],
                    s * 0.15,
                    color,
                );
            }
        }
        Icon::Attack => {
            // Sword: diagonal blade, tip, crossguard and short grip.
            segment(
                batch,
                atlas,
                [cx - s * 0.42, cy + s * 0.42],
                [cx + s * 0.42, cy - s * 0.42],
                s * 0.20,
                color,
            );
            tri(
                batch,
                atlas,
                [cx + s * 0.66, cy - s * 0.66],
                [cx + s * 0.31, cy - s * 0.48],
                [cx + s * 0.48, cy - s * 0.31],
                color,
            );
            segment(
                batch,
                atlas,
                [cx - s * 0.56, cy + s * 0.14],
                [cx - s * 0.12, cy + s * 0.58],
                s * 0.16,
                color,
            );
            segment(
                batch,
                atlas,
                [cx - s * 0.45, cy + s * 0.45],
                [cx - s * 0.66, cy + s * 0.66],
                s * 0.22,
                color,
            );
        }
        Icon::Use => {
            // Open hand/pointer. The taller index finger gives it a distinct
            // silhouette from attack even at the smallest phone GUI scale.
            let w = s * 0.18;
            batch.fill(atlas, cx - s * 0.28, cy - s * 0.05, s * 0.70, s * 0.55, color);
            batch.fill(atlas, cx - s * 0.25, cy - s * 0.60, w, s * 0.62, color);
            batch.fill(atlas, cx + s * 0.02, cy - s * 0.38, w, s * 0.42, color);
            batch.fill(atlas, cx + s * 0.29, cy - s * 0.26, w, s * 0.32, color);
            segment(
                batch,
                atlas,
                [cx - s * 0.34, cy + s * 0.33],
                [cx - s * 0.58, cy + s * 0.08],
                s * 0.18,
                color,
            );
        }
        Icon::Inventory => {
            // 3x3 inventory slots.
            let cell = s * 0.31;
            let gap = s * 0.10;
            let total = cell * 3.0 + gap * 2.0;
            let x0 = cx - total * 0.5;
            let y0 = cy - total * 0.5;
            for row in 0..3 {
                for col in 0..3 {
                    batch.fill(
                        atlas,
                        x0 + col as f32 * (cell + gap),
                        y0 + row as f32 * (cell + gap),
                        cell,
                        cell,
                        color,
                    );
                }
            }
        }
        Icon::Chat => {
            let w = s * 1.25;
            let h = s * 0.82;
            let x = cx - w * 0.5;
            let y = cy - h * 0.55;
            let line = (s * 0.14).max(1.2);
            batch.fill(atlas, x, y, w, line, color);
            batch.fill(atlas, x, y + h - line, w, line, color);
            batch.fill(atlas, x, y, line, h, color);
            batch.fill(atlas, x + w - line, y, line, h, color);
            tri(
                batch,
                atlas,
                [cx - s * 0.30, y + h - line],
                [cx - s * 0.54, cy + s * 0.66],
                [cx + s * 0.02, y + h - line],
                color,
            );
        }
        Icon::Pause => {
            let w = s * 0.28;
            batch.fill(atlas, cx - s * 0.48, cy - s * 0.62, w, s * 1.24, color);
            batch.fill(atlas, cx + s * 0.20, cy - s * 0.62, w, s * 1.24, color);
        }
    }
}
