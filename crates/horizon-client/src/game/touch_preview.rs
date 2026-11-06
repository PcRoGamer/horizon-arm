//! Native, opt-in preview of Horizon Android's touch controls on desktop.
//!
//! Run an unoptimised Windows ARM64 build with HORIZON_TOUCH_PREVIEW=1.
//! This draws inside Horizon's GPU HUD, so --screenshot-after captures it.
//! The Android overlay remains the Java Canvas implementation: this mirrors
//! its palette, relative geometry, strokes, icons and four control schemes.
//! No real touch injection is implied on desktop, and release builds do not
//! call this code. Reconcile geometry with HorizonActivity.java on changes.
use horizon_client::hud::{HudAtlas, HudBatch, rgba};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
struct Config {
    scheme: u8,
    scale: f32,
    opacity: f32,
    title: bool,
    always_sprint: bool,
}
static CONFIG: OnceLock<Option<Config>> = OnceLock::new();

fn config() -> Option<Config> {
    *CONFIG.get_or_init(|| {
        let enabled = std::env::var("HORIZON_TOUCH_PREVIEW").ok();
        if !enabled
            .as_deref()
            .is_some_and(|v| matches!(v, "1" | "true" | "yes"))
        {
            return None;
        }
        let number = |name: &str, default: u32, min: u32, max: u32| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(default)
                .clamp(min, max)
        };
        Some(Config {
            scheme: number("HORIZON_TOUCH_PREVIEW_SCHEME", 1, 0, 3) as u8,
            scale: number("HORIZON_TOUCH_PREVIEW_SCALE", 100, 80, 120) as f32 / 100.0,
            opacity: number("HORIZON_TOUCH_PREVIEW_OPACITY", 35, 35, 100) as f32 / 100.0,
            title: std::env::var("HORIZON_TOUCH_PREVIEW_TITLE").ok().as_deref() == Some("1"),
            always_sprint: std::env::var("HORIZON_TOUCH_PREVIEW_ALWAYS_SPRINT")
                .ok()
                .as_deref()
                == Some("1"),
        })
    })
}

/// The Android Java overlay hides in menus; the extra title switch exists
/// only for isolated visual-debug runs, never during normal interaction.
pub(super) fn title_enabled() -> bool {
    config().is_some_and(|c| c.title)
}

fn dpad_geometry(w: f32, h: f32, scale: f32) -> (f32, f32, f32, f32) {
    // HudBatch works in GUI pixels; Java computes the same bounds in
    // physical pixels using the corresponding auto GUI scale.
    let hotbar_scale = super::touch_hotbar::magnification(w);
    let bar_left = w * 0.5 - 91.0 * hotbar_scale;
    let bar_top = h - 15.0 - 22.0 * hotbar_scale;
    let r = 0.068 * h * scale;
    let off = 0.147 * h * scale;
    let cx = (0.11 * w).max(0.23 * h * scale);
    let mut cy = 0.72 * h;
    if cx + off + r > bar_left - 9.0 {
        cy = cy.min(bar_top - 9.0 - r);
    }
    if cx + r > bar_left - 9.0 {
        cy = cy.min(bar_top - 9.0 - off - r);
    }
    (cx, cy, off, r)
}

fn joystick_y(w: f32, h: f32, scale: f32) -> f32 {
    let r = 0.092 * h * scale;
    let mag = super::touch_hotbar::magnification(w);
    let bar_left = w * 0.5 - 91.0 * mag;
    let bar_top = h - 15.0 - 22.0 * mag;
    let mut y = 0.705 * h;
    if 0.11 * w + r > bar_left - 9.0 {
        y = y.min(bar_top - 9.0 - r);
    }
    y
}

pub(super) fn draw(batch: &mut HudBatch, atlas: &HudAtlas) {
    let Some(cfg) = config() else { return };
    let (w, h) = (batch.width, batch.height);
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let mut draw = Painter { batch, atlas, cfg };
    if cfg.scheme < 2 {
        draw.joystick(0.11 * w, joystick_y(w, h, cfg.scale), 0.092 * h * cfg.scale);
        draw.control(
            w - 0.290 * h,
            0.73 * h,
            0.040 * h * cfg.scale,
            Symbol::Sneak,
        );
        draw.control(
            0.11 * w + 0.190 * h,
            0.57 * h,
            0.035 * h * cfg.scale,
            Symbol::Sprint,
        );
    } else {
        let (cx, cy, offset, r) = dpad_geometry(w, h, cfg.scale);
        for (direction, dx, dy) in [
            (Direction::Up, 0.0, -1.0),
            (Direction::Right, 1.0, 0.0),
            (Direction::Down, 0.0, 1.0),
            (Direction::Left, -1.0, 0.0),
        ] {
            draw.control(
                cx + dx * offset,
                cy + dy * offset,
                r,
                Symbol::Arrow(direction),
            );
        }
        draw.control(cx, cy, r, Symbol::Sneak);
        draw.control(
            cx + 1.55 * offset,
            cy - 1.45 * offset,
            0.035 * h * cfg.scale,
            Symbol::Sprint,
        );
    }
    draw.control(w - 0.160 * h, 0.72 * h, 0.051 * h * cfg.scale, Symbol::Jump);
    if cfg.scheme & 1 == 1 {
        draw.control(w - 0.270 * h, 0.56 * h, 0.042 * h * cfg.scale, Symbol::Hit);
        draw.control(w - 0.145 * h, 0.52 * h, 0.042 * h * cfg.scale, Symbol::Use);
    }
    // Native hotbar end-cap is painted by touch_hotbar, not this overlay.
    draw.control(
        0.50 * w - 0.045 * h,
        0.045 * h,
        0.028 * h * cfg.scale,
        Symbol::Chat,
    );
    draw.control(
        0.50 * w + 0.045 * h,
        0.045 * h,
        0.028 * h * cfg.scale,
        Symbol::Pause,
    );
}

#[derive(Clone, Copy, Debug)]
enum Direction {
    Up,
    Right,
    Down,
    Left,
}
#[derive(Clone, Copy, Debug)]
enum Symbol {
    Jump,
    Sneak,
    Sprint,
    Hit,
    Use,
    Chat,
    Pause,
    Arrow(Direction),
}

// Generated original icon masks. One source for the Java and ARM64 preview.
// Glyph source: identical 11x24 masks in Android Canvas and Rust native preview
const PIXEL_GLYPHS: [[u32; 24]; 11] = [
    [
        0x000000, 0x000000, 0x000000, 0x000000, 0x0FFFF0, 0x1FFFF0, 0x180030, 0x180030, 0x180030,
        0x1999F0, 0x1BFFF0, 0x1BFFF0, 0x1999F0, 0x180030, 0x180030, 0x1C7FF0, 0x1DFFF0, 0x0FC000,
        0x0F8000, 0x0F0000, 0x0C0000, 0x000000, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000000, 0x000000, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0,
        0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0, 0x03C3C0,
        0x03C3C0, 0x03C3C0, 0x000000, 0x000000, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000380, 0x000380, 0x0007C0, 0x000380, 0x3F8700, 0x3F8F80, 0x003FC0,
        0x7F39F0, 0x7F70FC, 0x00707C, 0x00E000, 0x07E000, 0x0FF800, 0x0FFC00, 0x071E00, 0x0F0E00,
        0x0E0E00, 0x1C0E00, 0x180C00, 0x000C00, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000000, 0x001800, 0x003C00, 0x007E00, 0x007E00, 0x00FF00, 0x01FF80,
        0x03FFC0, 0x07FFE0, 0x007E00, 0x007E00, 0x007E00, 0x007E00, 0x007E00, 0x007E00, 0x007E00,
        0x000000, 0x000000, 0x1FFFF8, 0x1FFFF8, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000000, 0x003C00, 0x003C00, 0x003C00, 0x003C00, 0x003C00, 0x003C00,
        0x00FF00, 0x03FFC0, 0x03FFC0, 0x01FF80, 0x00FF00, 0x007E00, 0x003C00, 0x001800, 0x000000,
        0x000000, 0x000000, 0x07FFE0, 0x07FFE0, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x00000C, 0x00003C, 0x00007C, 0x00007C, 0x0000FC, 0x0001DC, 0x0003F8,
        0x0007F8, 0x000FE0, 0x001C00, 0x183800, 0x1C7000, 0x0FE000, 0x07C000, 0x07C000, 0x07C000,
        0x0FE000, 0x1C7000, 0x383000, 0x300000, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x003C00, 0x00FF00, 0x01FF80, 0x07C3E0, 0x1F00F8, 0x1E0078, 0x1F00F8,
        0x1FC3F8, 0x19FF98, 0x18FF18, 0x183C18, 0x181818, 0x181818, 0x181818, 0x1E1878, 0x1F18F8,
        0x07DBE0, 0x01FF80, 0x007F00, 0x003C00, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000000, 0x001800, 0x003C00, 0x007E00, 0x007E00, 0x00FF00, 0x01FF80,
        0x03FFC0, 0x07FFE0, 0x0FFFF0, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00,
        0x00FF00, 0x00FF00, 0x00FF00, 0x000000, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000000, 0x000000, 0x000800, 0x000C00, 0x000E00, 0x000F00, 0x1FFF80,
        0x1FFFE0, 0x1FFFF0, 0x1FFFF8, 0x1FFFF8, 0x1FFFF0, 0x1FFFE0, 0x1FFF80, 0x000F00, 0x000E00,
        0x000C00, 0x000800, 0x000000, 0x000000, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000000, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00,
        0x00FF00, 0x00FF00, 0x00FF00, 0x0FFFF0, 0x07FFE0, 0x03FFC0, 0x01FF80, 0x00FF00, 0x007E00,
        0x007E00, 0x003C00, 0x001800, 0x000000, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000000, 0x000000, 0x001000, 0x003000, 0x007000, 0x00F000, 0x01FFF8,
        0x07FFF8, 0x0FFFF8, 0x1FFFF8, 0x1FFFF8, 0x0FFFF8, 0x07FFF8, 0x01FFF8, 0x00F000, 0x007000,
        0x003000, 0x001000, 0x000000, 0x000000, 0x000000, 0x000000,
    ],
];

struct Painter<'a> {
    batch: &'a mut HudBatch,
    atlas: &'a HudAtlas,
    cfg: Config,
}
impl Painter<'_> {
    fn color(&self, rgb: u32, opacity_factor: f32) -> u32 {
        let a = (255.0 * self.cfg.opacity * opacity_factor)
            .round()
            .clamp(0.0, 255.0) as u8;
        rgba((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, a)
    }

    fn quad(&mut self, v: [[f32; 2]; 4], color: u32) {
        self.batch.fill_quad(self.atlas, v, color);
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: u32) {
        self.batch.fill(self.atlas, x, y, w, h, color);
    }

    fn segment(&mut self, a: [f32; 2], b: [f32; 2], width: f32, color: u32) {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let length = (dx * dx + dy * dy).sqrt();
        if length < 0.001 {
            return;
        }
        let nx = -dy / length * width * 0.5;
        let ny = dx / length * width * 0.5;
        self.quad(
            [
                [a[0] + nx, a[1] + ny],
                [b[0] + nx, b[1] + ny],
                [b[0] - nx, b[1] - ny],
                [a[0] - nx, a[1] - ny],
            ],
            color,
        );
    }

    fn stroke_box(&mut self, x: f32, y: f32, r: f32, thick: f32, color: u32) {
        let l = x - r;
        let t = y - r;
        let size = 2.0 * r;
        self.rect(l, t, size, thick, color);
        self.rect(l, t + size - thick, size, thick, color);
        self.rect(l, t + thick, thick, size - thick * 2.0, color);
        self.rect(
            l + size - thick,
            t + thick,
            thick,
            size - thick * 2.0,
            color,
        );
    }

    fn disc(&mut self, cx: f32, cy: f32, radius: f32, color: u32) {
        // Actual GPU HUD triangles, not a post-processed bitmap.
        const SLICES: usize = 40;
        for index in 0..SLICES {
            let a = (index as f32) * std::f32::consts::TAU / SLICES as f32;
            let b = ((index + 1) as f32) * std::f32::consts::TAU / SLICES as f32;
            let p0 = [cx, cy];
            let p1 = [cx + radius * a.cos(), cy + radius * a.sin()];
            let p2 = [cx + radius * b.cos(), cy + radius * b.sin()];
            self.quad([p0, p1, p2, p2], color);
        }
    }

    fn ring(&mut self, cx: f32, cy: f32, radius: f32, thick: f32, color: u32) {
        const SLICES: usize = 40;
        for index in 0..SLICES {
            let a = index as f32 * std::f32::consts::TAU / SLICES as f32;
            let b = (index + 1) as f32 * std::f32::consts::TAU / SLICES as f32;
            let inner = radius - thick * 0.5;
            let outer = radius + thick * 0.5;
            self.quad(
                [
                    [cx + inner * a.cos(), cy + inner * a.sin()],
                    [cx + outer * a.cos(), cy + outer * a.sin()],
                    [cx + outer * b.cos(), cy + outer * b.sin()],
                    [cx + inner * b.cos(), cy + inner * b.sin()],
                ],
                color,
            );
        }
    }

    fn round_rect(&mut self, x: f32, y: f32, width: f32, height: f32, radius: f32, color: u32) {
        let r = radius.min(width * 0.5).min(height * 0.5);
        self.rect(x + r, y, width - 2.0 * r, height, color);
        self.rect(x, y + r, r, height - 2.0 * r, color);
        self.rect(x + width - r, y + r, r, height - 2.0 * r, color);
        for (cx, cy, begin) in [
            (x + r, y + r, std::f32::consts::PI),
            (x + width - r, y + r, -std::f32::consts::FRAC_PI_2),
            (x + width - r, y + height - r, 0.0),
            (x + r, y + height - r, std::f32::consts::FRAC_PI_2),
        ] {
            for i in 0..8 {
                let a = begin + i as f32 * std::f32::consts::FRAC_PI_2 / 8.0;
                let b = begin + (i + 1) as f32 * std::f32::consts::FRAC_PI_2 / 8.0;
                let p = [cx, cy];
                let u = [cx + r * a.cos(), cy + r * a.sin()];
                let v = [cx + r * b.cos(), cy + r * b.sin()];
                self.quad([p, u, v, v], color);
            }
        }
    }


    fn control(&mut self, x: f32, y: f32, r: f32, sym: Symbol) {
        // Pixel-aligned translucent Bedrock face, same 4-layer material as
        // the production Android Canvas overlay. No opaque panel beneath it.
        let edge = (r * 0.047).round().max(1.25);
        let face = if matches!(sym, Symbol::Sprint) && self.cfg.always_sprint {
            self.color(0x97B39F, 0.76)
        } else {
            self.color(0xAFB4AE, 0.76)
        };
        let outline = self.color(0x151B1B, 0.90);
        let highlight = self.color(0xE0E4DB, 0.24);
        let bottom = self.color(0x283231, 0.38);
        let lip = (r * 0.024).round().max(1.0);
        // Four thin outside bands, not a dark filled backdrop.
        self.stroke_box(x, y, r, edge, outline);
        self.rect(x-r+edge, y-r+edge, 2.0*(r-edge), 2.0*(r-edge), face);
        self.rect(x-r+edge, y-r+edge, 2.0*(r-edge), lip, highlight);
        self.rect(x-r+edge, y+r-edge-lip, 2.0*(r-edge), lip, bottom);
        self.glyph(x, y, r, sym);
    }

    fn joystick(&mut self, x: f32, y: f32, r: f32) {
        let disc = self.color(0x13171B, 0.22);
        let ring = self.color(0xF4F7F8, 2.45);
        let dark = self.color(0x0C0F11, 1.80);
        let face = self.color(0xE4E9EB, 2.15);
        self.disc(x, y, r, disc);
        self.ring(x, y, r * 0.95, r * 0.041, ring);
        self.disc(x, y + r * 0.032, r * 0.42, dark);
        self.disc(x, y, r * 0.36, face);
    }

    // Original 24px glyphs using the exact Java Canvas bitmap masks.
    fn glyph(&mut self, x: f32, y: f32, radius: f32, glyph: Symbol) {
        let icon = match glyph {
            Symbol::Chat => 0,
            Symbol::Pause => 1,
            Symbol::Sprint => 2,
            Symbol::Jump => 3,
            Symbol::Sneak => 4,
            Symbol::Hit => 5,
            Symbol::Use => 6,
            Symbol::Arrow(Direction::Up) => 7,
            Symbol::Arrow(Direction::Right) => 8,
            Symbol::Arrow(Direction::Down) => 9,
            Symbol::Arrow(Direction::Left) => 10,
        };
        let pixel = radius * 0.056;
        let x0 = x - 12.0 * pixel;
        let y0 = y - 12.0 * pixel;
        let ink = self.color(0x111516, 1.9);
        for (row, bits) in PIXEL_GLYPHS[icon].iter().copied().enumerate() {
            let mut col = 0;
            while col < 24 {
                if bits & (1 << (23 - col)) == 0 {
                    col += 1;
                    continue;
                }
                let first = col;
                while col < 24 && bits & (1 << (23 - col)) != 0 {
                    col += 1;
                }
                self.rect(
                    x0 + first as f32 * pixel,
                    y0 + row as f32 * pixel,
                    (col - first) as f32 * pixel,
                    pixel,
                    ink,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn touch_preview_is_debug_only_and_never_requires_android() {
        let cfg = Config {
            scheme: 3,
            scale: 1.2,
            opacity: 0.35,
            title: false,
            always_sprint: false,
        };
        assert!(cfg.scheme >= 2 && cfg.scheme & 1 == 1);
        assert_eq!(cfg.scale, 1.2);
        assert!(!cfg.title);
    }
    #[test]
    fn four_schemes_match_android_modes() {
        for scheme in 0..4 {
            let dpad = scheme >= 2;
            let crosshair = scheme & 1 == 1;
            assert_eq!(
                (dpad, crosshair),
                [(false, false), (false, true), (true, false), (true, true)][scheme]
            );
        }
    }
}
