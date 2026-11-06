//! Offscreen capture of Horizon's REAL Rust HUD hotbar without running a world.
//!
//! Source of truth: hud_layout::hotbar, hud_layout::paint and game/touch_hotbar.
//! Uses the game's packed HudAtlas. CPU triangulation is only the output
//! transport, not a replacement GUI layout or a fake hotbar texture.
//!
//! HORIZON_TOUCH_PREVIEW=1 cargo run -p horizon-client --bin hud-offscreen \
//!   -- <resource-root> <output.png> 1200 630
#![forbid(unsafe_code)]
use horizon_client_render::{
    hud::{HudAtlas, HudBatch},
    hud_layout::{self, HotbarInputs},
};
use std::{env, error::Error, fs::File, path::Path};

#[path = "../game/touch_hotbar.rs"]
mod touch_hotbar;

fn edge(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (p[0] - a[0]) * (b[1] - a[1]) - (p[1] - a[1]) * (b[0] - a[0])
}

/// A nearest-neighbour CPU interpreter for the actual renderer's textured
/// triangles. This intentionally uses the exact HudBatch vertices and atlas
/// texels generated in production. No hand-drawn slots or fake inventory.
fn rasterize(batch: &HudBatch, atlas: &HudAtlas, size: [u32; 2]) -> Vec<u8> {
    let [width, height] = size;
    let mut out = vec![0u8; (width as usize) * (height as usize) * 4];
    let pixels = atlas.pixels();
    let atlas_rgba = pixels.as_ref();
    let scale = batch.scale;
    for triangle in batch.vertices.chunks_exact(3) {
        let vertices: [[f32; 2]; 3] =
            std::array::from_fn(|i| [triangle[i].pos[0] * scale, triangle[i].pos[1] * scale]);
        let area = edge(vertices[0], vertices[1], vertices[2]);
        if area.abs() < 0.00001 {
            continue;
        }
        let min_x = (vertices
            .iter()
            .map(|v| v[0])
            .fold(f32::INFINITY, f32::min)
            .floor() as i32)
            .clamp(0, width as i32);
        let max_x = (vertices
            .iter()
            .map(|v| v[0])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil() as i32)
            .clamp(0, width as i32);
        let min_y = (vertices
            .iter()
            .map(|v| v[1])
            .fold(f32::INFINITY, f32::min)
            .floor() as i32)
            .clamp(0, height as i32);
        let max_y = (vertices
            .iter()
            .map(|v| v[1])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil() as i32)
            .clamp(0, height as i32);
        let tint = triangle[0].color.to_le_bytes(); // HudBatch::rgba -> R,G,B,A
        for y in min_y..max_y {
            for x in min_x..max_x {
                let center = [x as f32 + 0.5, y as f32 + 0.5];
                let weights = [
                    edge(vertices[1], vertices[2], center) / area,
                    edge(vertices[2], vertices[0], center) / area,
                    edge(vertices[0], vertices[1], center) / area,
                ];
                // Top-left shared edges have practically zero coverage:
                // slight negative tolerance avoids one-pixel cracks.
                if weights.iter().any(|w| *w < -0.000001) {
                    continue;
                }
                let u = (0..3).map(|i| weights[i] * triangle[i].uv[0]).sum::<f32>();
                let v = (0..3).map(|i| weights[i] * triangle[i].uv[1]).sum::<f32>();
                let tx = (u * atlas.width as f32).floor() as i32;
                let ty = (v * atlas.height as f32).floor() as i32;
                let tx = tx.clamp(0, atlas.width as i32 - 1) as u32;
                let ty = ty.clamp(0, atlas.height as i32 - 1) as u32;
                let src_idx = ((ty * atlas.width + tx) * 4) as usize;
                let dst_idx = ((y as u32 * width + x as u32) * 4) as usize;
                let src_a = atlas_rgba[src_idx + 3] as f32 * tint[3] as f32 / (255.0 * 255.0);
                if src_a <= 0.0001 {
                    continue;
                }
                let dst_a = out[dst_idx + 3] as f32 / 255.0;
                let out_a = src_a + dst_a * (1.0 - src_a);
                if out_a <= 0.0001 {
                    continue;
                }
                for channel in 0..3 {
                    let src = atlas_rgba[src_idx + channel] as f32 * tint[channel] as f32 / 255.0;
                    let dst = out[dst_idx + channel] as f32;
                    out[dst_idx + channel] = ((src * src_a + dst * dst_a * (1.0 - src_a)) / out_a)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
                out[dst_idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 5 {
        eprintln!("Usage: hud-offscreen <resource-root> <output.png> <width> <height>");
        return Err("Expected four arguments".into());
    }
    let root = Path::new(&args[1]);
    let path = Path::new(&args[2]);
    let width: u32 = args[3].parse()?;
    let height: u32 = args[4].parse()?;
    if width < 320 || height < 240 || width > 4096 || height > 4096 {
        return Err("Invalid screenshot size".into());
    }
    if !touch_hotbar::enabled() {
        return Err("Set HORIZON_TOUCH_PREVIEW=1 and run a debug build".into());
    }
    let atlas = HudAtlas::build(root)?;
    let have_hotbar = atlas.sprites.contains_key("gui/sprites/hud/hotbar");
    let have_selection = atlas
        .sprites
        .contains_key("gui/sprites/hud/hotbar_selection");
    if !(have_hotbar && have_selection) {
        return Err(format!(
            "Missing real hotbar sprites (bar={have_hotbar}, selection={have_selection}); \
            check the resource root: {}",
            root.display()
        )
        .into());
    }
    let mut batch = HudBatch::new([width, height]);
    let first = batch.vertices.len();
    let draws = hud_layout::hotbar(
        batch.width as i32,
        batch.height as i32,
        &HotbarInputs {
            selected: 0,
            right_handed: true,
            offhand: false,
            slots: [None; 9],
            offhand_pop: 0.0,
            attack: None,
        },
    );
    hud_layout::paint(&mut batch, &atlas, &draws);
    touch_hotbar::transform_hotbar(&mut batch, first);
    touch_hotbar::inventory_cap(&mut batch, &atlas);
    let output = rasterize(&batch, &atlas, [width, height]);
    if output.chunks_exact(4).filter(|px| px[3] > 0).count() < 400 {
        return Err("Rendered hotbar is nearly empty".into());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = File::create(path)?;
    let mut writer = png::Encoder::new(file, width, height);
    writer.set_color(png::ColorType::Rgba);
    writer.set_depth(png::BitDepth::Eight);
    writer.write_header()?.write_image_data(&output)?;
    println!(
        "HUD_OFFSCREEN_OK path={} size={}x{} gui={}x{} gui_scale={} vertices={}",
        path.display(),
        width,
        height,
        batch.width,
        batch.height,
        batch.scale,
        batch.vertices.len()
    );
    Ok(())
}
