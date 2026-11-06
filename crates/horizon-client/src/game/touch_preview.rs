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
            scheme: number("HORIZON_TOUCH_PREVIEW_SCHEME", 2, 0, 3) as u8,
            scale: number("HORIZON_TOUCH_PREVIEW_SCALE", 100, 80, 120) as f32 / 100.0,
            opacity: number("HORIZON_TOUCH_PREVIEW_OPACITY", 72, 35, 100) as f32 / 100.0,
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
        // Four-way D-pad stays uncluttered; actions are on the right.
        draw.control(
            w - (if cfg.scheme & 1 == 1 { 0.40 } else { 0.29 }) * h,
            (if cfg.scheme & 1 == 1 { 0.45 } else { 0.55 }) * h,
            0.035 * h * cfg.scale,
            Symbol::Sprint,
        );
        draw.control(w - 0.290 * h, 0.73 * h, 0.040 * h * cfg.scale, Symbol::Sneak);
    }
    draw.control(w - 0.133 * h, 0.72 * h, 0.066 * h * cfg.scale, Symbol::Jump);
    if cfg.scheme & 1 == 1 {
        draw.control(w - 0.270 * h, 0.56 * h, 0.042 * h * cfg.scale, Symbol::Hit);
        draw.control(w - 0.145 * h, 0.52 * h, 0.042 * h * cfg.scale, Symbol::Use);
    }
    // Native hotbar end-cap is painted by touch_hotbar, not this overlay.
    draw.control(
        0.50 * w + 0.050 * h,
        0.045 * h,
        0.028 * h * cfg.scale,
        Symbol::Chat,
    );
    draw.control(
        0.50 * w + (0.050 + 0.067 * cfg.scale) * h,
        0.045 * h,
        0.028 * h * cfg.scale,
        Symbol::Pause,
    );
    draw.control(
        0.50 * w + (0.050 - 0.067 * cfg.scale) * h,
        0.045 * h,
        0.028 * h * cfg.scale,
        Symbol::Perspective,
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
    Perspective,
    Arrow(Direction),
}

// Twelve original 24px controller glyphs, shared with Android Canvas.
const PIXEL_GLYPHS: [[u32; 24]; 12] = [
    [
        0x000000, 0x000000, 0x000000, 0x0FFFF8, 0x1FFFF8, 0x180018, 0x180018, 0x180018, 0x180018,
        0x188898, 0x180018, 0x180018, 0x180018, 0x180018, 0x180018, 0x1E7FF8, 0x1EFFF8, 0x07C000,
        0x0F8000, 0x0F0000, 0x0E0000, 0x0C0000, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000000, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0,
        0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0, 0x0781E0,
        0x0781E0, 0x0781E0, 0x0781E0, 0x000000, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x0003C0, 0x0003C0, 0x0003C0, 0x0003C0, 0x000000, 0x000400, 0x3C0E00,
        0x3C7E00, 0x00FFC0, 0x00FFE0, 0x787FE0, 0x787E00, 0x00FF80, 0x00EFC0, 0x19E3E0, 0x1BC0F0,
        0x038070, 0x018078, 0x000038, 0x000030, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000000, 0x001800, 0x003C00, 0x007E00, 0x00FF00, 0x01FF80, 0x03FFC0,
        0x07FFE0, 0x0FFFF0, 0x1FFFF8, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00, 0x00FF00,
        0x000000, 0x000000, 0x000000, 0x1FFFF8, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000000, 0x0003C0, 0x0003C0, 0x0003C0, 0x0003C0, 0x000000, 0x000600,
        0x001F00, 0x003F80, 0x00FFE0, 0x01FBE0, 0x01F000, 0x03FE00, 0x07FFF0, 0x0FBFF0, 0x0FC7F0,
        0x0FE000, 0x07E000, 0x006000, 0x3FFFFC, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000030, 0x000078, 0x0000F8, 0x0001F8, 0x0003F0, 0x0007E0, 0x000FC0,
        0x001F80, 0x003F00, 0x01FE00, 0x01FC00, 0x00F800, 0x01F000, 0x03F800, 0x07DC00, 0x0F8800,
        0x1F0000, 0x3E0000, 0x3C0000, 0x180000, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x001800, 0x007E00, 0x00F780, 0x03C3C0, 0x0F00F0, 0x3C003C, 0x3C003C, 0x3F007C,
        0x3781EC, 0x31E78C, 0x307E0C, 0x301C0C, 0x30180C, 0x30180C, 0x30180C, 0x30180C, 0x3C183C,
        0x0F18F8, 0x07D9E0, 0x01FF80, 0x007E00, 0x001800, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000000, 0x000800, 0x001800, 0x003C00, 0x003E00, 0x007E00, 0x00FF00,
        0x00FF80, 0x01FF80, 0x03FFC0, 0x03FFE0, 0x07FFE0, 0x0FFFF0, 0x0FFFF8, 0x1FFFF8, 0x3FFFFC,
        0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x040000, 0x070000, 0x078000, 0x07E000, 0x07F000, 0x07FC00, 0x07FF00,
        0x07FF80, 0x07FFE0, 0x07FFF8, 0x07FFF8, 0x07FFE0, 0x07FFC0, 0x07FF00, 0x07FC00, 0x07F800,
        0x07E000, 0x07C000, 0x070000, 0x040000, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x3FFFFC, 0x1FFFF8, 0x1FFFF8,
        0x0FFFF0, 0x07FFE0, 0x07FFE0, 0x03FFC0, 0x01FF80, 0x01FF80, 0x00FF00, 0x007E00, 0x007E00,
        0x003C00, 0x001800, 0x001800, 0x000000, 0x000000, 0x000000,
    ],
    [
        0x000000, 0x000000, 0x000020, 0x000060, 0x0001E0, 0x0007E0, 0x000FE0, 0x003FE0, 0x007FE0,
        0x01FFE0, 0x07FFE0, 0x0FFFE0, 0x1FFFE0, 0x07FFE0, 0x01FFE0, 0x00FFE0, 0x003FE0, 0x001FE0,
        0x0007E0, 0x0001E0, 0x0000E0, 0x000020, 0x000000, 0x000000,
    ],
    [
        0x001800, 0x007E00, 0x01E780, 0x0781E0, 0x1E0078, 0x38001E, 0x78001E, 0x7E007E, 0x6781E6,
        0x61E786, 0x607E06, 0x601806, 0x601806, 0x601806, 0x601806, 0x601806, 0x601806, 0x601806,
        0x78181E, 0x1E1878, 0x0799E0, 0x01FF80, 0x007E00, 0x001800,
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

    // Deterministic rounded-rect gradient, emitted as ordinary HUD triangles.
    // This retains GUI-pixel geometry in the actual game renderer.
    fn chrome_gradient(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        top: u32,
        bottom: u32,
        opacity: f32,
    ) {
        let steps = 32;
        for i in 0..steps {
            let v = (i as f32 + 0.5) / steps as f32;
            let dy = v * h;
            let corner_y = if dy < radius {
                radius - dy
            } else if dy > h - radius {
                dy - (h - radius)
            } else {
                0.0
            };
            let cut = if corner_y > 0.0 {
                radius - (radius * radius - corner_y * corner_y).max(0.0).sqrt()
            } else {
                0.0
            };
            let channel = |shift: u32| {
                let a = ((top >> shift) & 0xff) as f32;
                let b = ((bottom >> shift) & 0xff) as f32;
                (a + (b - a) * v).round() as u32
            };
            let rgb = (channel(16) << 16) | (channel(8) << 8) | channel(0);
            let ink = self.color(rgb, opacity);
            self.rect(
                x + cut,
                y + i as f32 * h / steps as f32,
                (w - 2.0 * cut).max(0.0),
                h / steps as f32 + 0.015,
                ink,
            );
        }
    }

    fn chrome_button(&mut self, x: f32, y: f32, r: f32, pressed: bool) {
        let bezel = (r * 0.054).max(1.2);
        let corner = (r * 0.255).max(3.0);
        let shift = if pressed { (r * 0.025).max(1.0) } else { 0.0 };
        let cy = y + shift;
        let shadow = self.color(0x111A24, 1.15);
        let outline = self.color(0x293746, 1.25);
        let rim = self.color(0xDCE7F1, 1.12);
        self.round_rect(x - r, cy - r + bezel * 0.66, 2.0 * r, 2.0 * r, corner, shadow);
        self.round_rect(x - r, cy - r, 2.0 * r, 2.0 * r, corner, outline);
        let inner = (corner - bezel).max(2.0);
        self.round_rect(
            x - r + bezel,
            cy - r + bezel * 0.82,
            2.0 * (r - bezel),
            2.0 * r - bezel * 1.84,
            inner,
            rim,
        );
        let face = bezel * 1.75;
        let face_corner = (corner - face).max(2.0);
        self.chrome_gradient(
            x - r + face,
            cy - r + face,
            2.0 * (r - face),
            2.0 * (r - face),
            face_corner,
            if pressed { 0xA2B1BE } else { 0xDDE9F4 },
            if pressed { 0x778999 } else { 0x9EADC2 },
            0.99,
        );
        // Fine light specular rim and lower inner seat. Unlike a block
        // texture, every pixel here describes a control surface.
        let glow = self.color(0xF6FCFF, if pressed { 0.28 } else { 0.95 });
        let seat = self.color(0x405062, 0.68);
        let e = face + bezel * 0.37;
        self.round_rect(
            x - r + e,
            cy - r + e,
            2.0 * (r - e),
            2.0 * (r - e),
            (face_corner - bezel * 0.3).max(1.0),
            glow,
        );
        // Clear the middle of the rim with a smaller gradient so the highlight
        // is a hairline rather than an opaque white slab.
        let clear = e + (bezel * 0.42).max(0.8);
        self.chrome_gradient(
            x - r + clear,
            cy - r + clear,
            2.0 * (r - clear),
            2.0 * (r - clear),
            (face_corner - bezel * 0.8).max(1.0),
            if pressed { 0xA2B1BE } else { 0xDDE9F4 },
            if pressed { 0x778999 } else { 0x9EADC2 },
            0.99,
        );
        // The dark perimeter seam is intentionally subtle.
        let seam = bezel * 0.65;
        let seam_w = (bezel * 0.38).max(0.5);
        self.round_rect(
            x - r + seam,
            cy - r + seam,
            2.0 * (r - seam),
            2.0 * (r - seam),
            (corner - seam).max(1.0),
            seat,
        );
        self.chrome_gradient(
            x - r + seam + seam_w,
            cy - r + seam + seam_w,
            2.0 * (r - seam - seam_w),
            2.0 * (r - seam - seam_w),
            (corner - seam - seam_w).max(1.0),
            if pressed { 0xA2B1BE } else { 0xDDE9F4 },
            if pressed { 0x778999 } else { 0x9EADC2 },
            0.99,
        );
    }

    fn control(&mut self, x: f32, y: f32, r: f32, sym: Symbol) {
        let pressed = matches!(sym, Symbol::Sprint) && self.cfg.always_sprint;
        self.chrome_button(x, y, r, pressed);
        self.glyph(x, y + if pressed { (r * 0.025).max(1.0) } else { 0.0 }, r, sym);
    }

    fn joystick(&mut self, x: f32, y: f32, r: f32) {
        let disc = self.color(0x1B2B39, 0.54);
        let ring = self.color(0xB8C9D8, 1.15);
        let groove = self.color(0x6D8398, 0.65);
        let dark = self.color(0x1C2B3B, 1.20);
        let face = self.color(0xDAE5EF, 1.22);
        let trim = self.color(0x8193A8, 0.80);
        self.disc(x, y, r, disc);
        self.ring(x, y, r * 0.91, r * 0.055, ring);
        self.ring(x, y, r * 0.78, r * 0.016, groove);
        self.disc(x, y + r * 0.04, r * 0.40, dark);
        self.disc(x, y, r * 0.34, face);
        self.ring(x, y, r * 0.33, r * 0.035, trim);
    }

    // Original 24px glyphs using the exact Java Canvas bitmap masks.
    fn glyph(&mut self, x: f32, y: f32, radius: f32, glyph: Symbol) {
        let icon = match glyph {
            Symbol::Chat => 0,
            Symbol::Pause => 1,
            Symbol::Perspective => 11,
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
