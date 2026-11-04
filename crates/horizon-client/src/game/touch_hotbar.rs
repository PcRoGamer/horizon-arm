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
    let x = batch.width * 0.5 + 91.0 * s - 1.0;
    let y = batch.height - BOTTOM_GAP - side;
    // Solid shared edge and three-step Minecraft pixel bevel.
    batch.fill(atlas, x, y, INVENTORY_CAP * s, side, rgba(17, 20, 22, 235));
    batch.fill(
        atlas,
        x + 2.0,
        y + 2.0,
        INVENTORY_CAP * s - 4.0,
        side - 4.0,
        rgba(92, 98, 100, 230),
    );
    batch.fill(
        atlas,
        x + 4.0,
        y + 4.0,
        INVENTORY_CAP * s - 8.0,
        side - 8.0,
        rgba(174, 183, 182, 242),
    );
    let unit = (side * 0.095).max(1.0);
    let ix = x + (INVENTORY_CAP * s - 5.0 * unit) * 0.5;
    let iy = y + (side - 5.0 * unit) * 0.5;
    // Nine inventory slots as a sharp, reproducible, resolution-independent icon.
    for row in 0..3 {
        for col in 0..3 {
            let px = ix + col as f32 * (unit * 1.6);
            let py = iy + row as f32 * (unit * 1.6);
            batch.fill(atlas, px, py, unit * 1.2, unit * 1.2, rgba(31, 39, 42, 255));
        }
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
