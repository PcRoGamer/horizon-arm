//! Mobile gameplay hotbar. The *real* vanilla hotbar/item vertices are
//! transformed together. No duplicate/fake inventory and no modified desktop
//! renderer unless the opt-in Windows ARM64 visual debugger is enabled.
use horizon_client::hud::{HudAtlas, HudBatch, rgba};

pub(super) const MAGNIFICATION: f32 = 1.36;
pub(super) const BOTTOM_GAP: f32 = 15.0;
pub(super) const STAT_LIFT: f32 = 20.0;
pub(super) const INVENTORY_CAP: f32 = 20.0;

/// Cap the Bedrock-matched 1.30 magnification at the minimum GUI width.
/// The 10 GUI-pixel edge clearance remains a hard invariant.
pub(super) fn magnification(width: f32) -> f32 {
    MAGNIFICATION.min((width * 0.5 - 10.0) / (91.0 + INVENTORY_CAP))
}

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
    let s = magnification(batch.width);
    for vertex in &mut batch.vertices[first..] {
        vertex.pos[0] = cx + (vertex.pos[0] - cx) * s;
        vertex.pos[1] = bottom + (vertex.pos[1] - batch.height) * s;
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

/// Scale/raise the real contextual experience bar with the mobile hotbar,
/// keeping the vanilla GUI draw calls and progression sprites unchanged.
pub(super) fn transform_experience(batch: &mut HudBatch, first: usize) {
    if !enabled() {
        return;
    }
    let s = magnification(batch.width);
    let cx = batch.width * 0.5;
    // ExperienceBar.extractBackground emits its sprite at GUI h - 29.
    let original_top = batch.height - 29.0;
    let target_top = batch.height - BOTTOM_GAP - 22.0 * s - 9.0;
    for vertex in &mut batch.vertices[first..] {
        vertex.pos[0] = cx + (vertex.pos[0] - cx) * s;
        vertex.pos[1] = target_top + (vertex.pos[1] - original_top) * s;
    }
}

/// Bedrock's survival HUD uses opposite TOP corners for health/armour and
/// hunger/air rather than Java's rows above the item hotbar.
///
/// The source UI has already emitted the real atlas sprites into HudBatch.
/// Relocate those vertices only, preserving their art, animation and order.
/// A left/right split at the GUI centre is safe for vanilla's health rows,
/// which are entirely in separate halves of the original GUI.
pub(super) fn anchor_health_to_top(batch: &mut HudBatch, first: usize) {
    if !enabled() || first >= batch.vertices.len() {
        return;
    }
    let center = batch.width * 0.5;
    let mut bounds = [
        [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY],
        [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY],
    ];
    for vertex in &batch.vertices[first..] {
        let side = usize::from(vertex.pos[0] >= center);
        let b = &mut bounds[side];
        b[0] = b[0].min(vertex.pos[0]);
        b[1] = b[1].min(vertex.pos[1]);
        b[2] = b[2].max(vertex.pos[0]);
    }
    // Bedrock's status icons are larger than Java Edition's default 9px
    // sprites. Scale actual atlas vertices, not replacement bitmap graphics.
    const STATUS_SCALE: f32 = 1.25;
    const INSET: f32 = 4.0;
    const TOP: f32 = 3.0;
    for vertex in &mut batch.vertices[first..] {
        let side = usize::from(vertex.pos[0] >= center);
        let [min_x, min_y, max_x] = bounds[side];
        if !min_x.is_finite() {
            continue;
        }
        vertex.pos[0] = if side == 0 {
            INSET + (vertex.pos[0] - min_x) * STATUS_SCALE
        } else {
            batch.width - INSET - (max_x - vertex.pos[0]) * STATUS_SCALE
        };
        vertex.pos[1] = TOP + (vertex.pos[1] - min_y) * STATUS_SCALE;
    }
}

/// Inventory is attached to the hotbar's right border, sized as a tenth
/// (end-cap) cell. The press/click region lives in horizon-winit::touch_gestures.
pub(super) fn inventory_cap(batch: &mut HudBatch, atlas: &HudAtlas) {
    if !enabled() {
        return;
    }
    let s = magnification(batch.width);
    let side = 22.0 * s;
    let cap = INVENTORY_CAP * s;
    let x = batch.width * 0.5 + 91.0 * s - 1.0;
    let y = batch.height - BOTTOM_GAP - side;
    // Integrated chrome: native nine-slot sprites remain untouched beneath
    // this end cap, while its icon and material match the mobile controls.
    batch.fill(atlas, x, y + 0.8 * s, cap, side, rgba(17, 26, 36, 245));
    batch.fill(atlas, x, y, cap, side - 0.8 * s, rgba(42, 55, 70, 248));
    batch.fill(atlas, x + 1.0 * s, y + 0.8 * s, cap - 2.0 * s, side - 2.4 * s,
        rgba(204, 218, 232, 245));
    let inner_x = x + 2.3 * s;
    let inner_y = y + 2.1 * s;
    let inner_w = cap - 4.6 * s;
    let inner_h = side - 4.7 * s;
    for row in 0..10 {
        let t = row as f32 / 9.0;
        let r = (212.0 - 52.0 * t) as u8;
        let g = (224.0 - 52.0 * t) as u8;
        let b = (237.0 - 40.0 * t) as u8;
        batch.fill(atlas, inner_x, inner_y + inner_h * t, inner_w, inner_h / 9.0 + 0.02,
            rgba(r, g, b, 244));
    }
    // Compact backpack silhouette: outline, lid, clasp and carry handle.
    let bx = x + cap * 0.28;
    let by = y + side * 0.36;
    let bw = cap * 0.44;
    let bh = side * 0.38;
    let ink = rgba(42, 52, 65, 254);
    let white = rgba(235, 242, 247, 252);
    let dark = rgba(105, 121, 138, 252);
    batch.fill(atlas, bx, by, bw, bh, ink);
    batch.fill(atlas, bx + 0.9 * s, by + 0.9 * s, bw - 1.8 * s, bh - 1.8 * s, white);
    batch.fill(atlas, bx + 1.0 * s, by + bh * 0.45, bw - 2.0 * s, 1.0 * s, dark);
    batch.fill(atlas, bx + bw * 0.40, by + bh * 0.36, bw * 0.20, bh * 0.29, ink);
    // Squared carry strap, drawn with the same 1px inventory icon grid.
    batch.fill(atlas, bx + bw * 0.25, by - 2.2 * s, bw * 0.50, 0.9 * s, ink);
    batch.fill(atlas, bx + bw * 0.25, by - 2.2 * s, 0.9 * s, 2.8 * s, ink);
    batch.fill(atlas, bx + bw * 0.75 - 0.9 * s, by - 2.2 * s,
        0.9 * s, 2.8 * s, ink);
    // One coherent hotbar rail: soft top reflection and lower shadow.
    let bar_x = batch.width * 0.5 - 91.0 * s;
    let bar_w = 182.0 * s;
    batch.fill(atlas, bar_x, y, bar_w, 0.52 * s, rgba(214, 226, 238, 174));
    batch.fill(atlas, bar_x, y + side - 0.76 * s, bar_w, 0.76 * s,
        rgba(25, 33, 42, 160));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mobile_geometry_fits_minimum_gui_width() {
        // 320x240 is the minimum GUI size used by Horizon's auto GUI scale.
        for width in [320.0, 360.0, 426.0, 480.0, 640.0] {
            let s = magnification(width);
            let right = width * 0.5 + (91.0 + INVENTORY_CAP) * s;
            assert!(right <= width - 10.0, "inventory cap clips at {width}");
            let left = width * 0.5 - 91.0 * s;
            assert!(left > 10.0);
        }
    }
}
