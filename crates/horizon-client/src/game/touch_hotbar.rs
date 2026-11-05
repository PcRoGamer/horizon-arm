//! Mobile gameplay hotbar. The *real* vanilla hotbar/item vertices are
//! transformed together. No duplicate/fake inventory and no modified desktop
//! renderer unless the opt-in Windows ARM64 visual debugger is enabled.
use horizon_client::hud::{HudAtlas, HudBatch, rgba};

pub(super) const MAGNIFICATION: f32 = 1.22;
pub(super) const BOTTOM_GAP: f32 = 15.0;
pub(super) const STAT_LIFT: f32 = 20.0;
pub(super) const INVENTORY_CAP: f32 = 27.0;

pub(super) fn enabled() -> bool {
    cfg!(target_os = "android")
        || (cfg!(debug_assertions)
            && std::env::var("HORIZON_TOUCH_PREVIEW")
                .ok()
                .is_some_and(|s| matches!(s.as_str(), "1" | "true" | "yes")))
}

/// Apply the same affine transform to *sprites, selection and item vertices*.
/// Coordinates are GUI pixels (the engine handles physical DPI separately).
pub(super) fn transform_hotbar(batch: &mut HudBatch, first: usize) {
    if !enabled() {
        return;
    }
    let (cx, bottom) = (batch.width * 0.5, batch.height - BOTTOM_GAP);
    for vertex in &mut batch.vertices[first..] {
        vertex.pos[0] = cx + (vertex.pos[0] - cx) * MAGNIFICATION;
        vertex.pos[1] = bottom + (vertex.pos[1] - batch.height) * MAGNIFICATION;
    }
}

pub(super) fn lift_stats(batch: &mut HudBatch, first: usize) {
    if !enabled() {
        return;
    }
    for vertex in &mut batch.vertices[first..] {
        vertex.pos[1] -= STAT_LIFT;
    }
}

/// Inventory is attached to the hotbar's right border, sized as a tenth
/// (end-cap) cell. The press/click region lives in horizon-winit::touch_gestures.
pub(super) fn inventory_cap(batch: &mut HudBatch, atlas: &HudAtlas) {
    if !enabled() {
        return;
    }
    let s = MAGNIFICATION;
    let side = 22.0 * s;
    let cap = INVENTORY_CAP * s;
    let x = batch.width * 0.5 + 91.0 * s - 1.0;
    let y = batch.height - BOTTOM_GAP - side;
    // Bedrock-style ellipsis is a muted extension of the hotbar, not an
    // independently raised 3x3 app-grid button.
    batch.fill(atlas, x, y, cap, side, rgba(11, 15, 16, 231));
    batch.fill(atlas, x + 1.5 * s, y + 1.5 * s,
               cap - 3.0 * s, side - 3.0 * s, rgba(57, 63, 64, 203));
    batch.fill(atlas, x + 2.5 * s, y + 2.5 * s,
               cap - 5.0 * s, side - 5.0 * s, rgba(102, 108, 106, 119));
    batch.fill(atlas, x + 2.5 * s, y + side - 3.3 * s,
               cap - 5.0 * s, 0.8 * s, rgba(29, 34, 36, 226));
    let dot = 3.5 * s;
    let gap = 2.5 * s;
    let total = 3.0 * dot + 2.0 * gap;
    let dx = x + (cap - total) * 0.5;
    let dy = y + (side - dot) * 0.5;
    for i in 0..3 {
        batch.fill(atlas, dx + i as f32 * (dot + gap), dy,
                   dot, dot, rgba(219, 227, 224, 235));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mobile_geometry_fits_minimum_gui_width() {
        // 320x240 is the minimum GUI size used by Horizon's auto GUI scale.
        for width in [320.0, 360.0, 426.0, 480.0, 640.0] {
            let right = width * 0.5 + (91.0 + INVENTORY_CAP) * MAGNIFICATION;
            assert!(right <= width - 10.0, "inventory cap clips at {width}");
            let left = width * 0.5 - 91.0 * MAGNIFICATION;
            assert!(left > 10.0);
        }
    }
}
