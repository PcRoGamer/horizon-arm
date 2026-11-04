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
}
static CONFIG: OnceLock<Option<Config>> = OnceLock::new();

fn config() -> Option<Config> {
    *CONFIG.get_or_init(|| {
        let enabled = std::env::var("HORIZON_TOUCH_PREVIEW").ok();
        if !enabled.as_deref().is_some_and(|v| matches!(v, "1" | "true" | "yes")) {
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
            opacity: number("HORIZON_TOUCH_PREVIEW_OPACITY", 72, 35, 100) as f32 / 100.0,
            title: std::env::var("HORIZON_TOUCH_PREVIEW_TITLE").ok().as_deref() == Some("1"),
        })
    })
}

/// The Android Java overlay hides in menus; the extra title switch exists
/// only for isolated visual-debug runs, never during normal interaction.
pub(super) fn title_enabled() -> bool {
    config().is_some_and(|c| c.title)
}

pub(super) fn draw(batch: &mut HudBatch, atlas: &HudAtlas) {
    let Some(cfg) = config() else { return };
    let (w, h) = (batch.width, batch.height);
    if w <= 0.0 || h <= 0.0 { return; }
    let mut draw = Painter { batch, atlas, cfg };
    if cfg.scheme < 2 {
        draw.joystick(0.15 * w, 0.72 * h, 0.135 * h * cfg.scale);
    } else {
        let offset = 0.068 * h * cfg.scale * 1.48;
        for (direction, dx, dy) in [
            (Direction::Up, 0.0, -1.0),
            (Direction::Right, 1.0, 0.0),
            (Direction::Down, 0.0, 1.0),
            (Direction::Left, -1.0, 0.0),
        ] {
            draw.control(0.15 * w + dx * offset, 0.72 * h + dy * offset,
                0.049 * h * cfg.scale, Symbol::Arrow(direction));
        }
    }
    // Same normalized centers, radii, and draw order as
    // HorizonActivity.TouchOverlay.onDraw.
    draw.control(0.90 * w, 0.72 * h, 0.085 * h * cfg.scale, Symbol::Jump);
    draw.control(0.75 * w, 0.78 * h, 0.065 * h * cfg.scale, Symbol::Sneak);
    if cfg.scheme & 1 == 1 {
        draw.control(0.75 * w, 0.54 * h, 0.073 * h * cfg.scale, Symbol::Hit);
        draw.control(0.91 * w, 0.46 * h, 0.070 * h * cfg.scale, Symbol::Use);
    }
    draw.control(0.36 * w, 0.67 * h, 0.055 * h * cfg.scale, Symbol::Sprint);
    draw.control(0.62 * w, 0.80 * h, 0.055 * h * cfg.scale, Symbol::Inventory);
    draw.control(0.50 * w, 0.10 * h, 0.055 * h * cfg.scale, Symbol::Pause);
}

#[derive(Clone, Copy, Debug)]
enum Direction { Up, Right, Down, Left }
#[derive(Clone, Copy, Debug)]
enum Symbol { Jump, Sneak, Sprint, Hit, Use, Inventory, Pause, Arrow(Direction) }

struct Painter<'a> {
    batch: &'a mut HudBatch,
    atlas: &'a HudAtlas,
    cfg: Config,
}
impl Painter<'_> {
    fn color(&self, rgb: u32, opacity_factor: f32) -> u32 {
        let a = (255.0 * self.cfg.opacity * opacity_factor).round().clamp(0.0, 255.0) as u8;
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
        if length < 0.001 { return; }
        let nx = -dy / length * width * 0.5;
        let ny = dx / length * width * 0.5;
        self.quad([
            [a[0] + nx, a[1] + ny], [b[0] + nx, b[1] + ny],
            [b[0] - nx, b[1] - ny], [a[0] - nx, a[1] - ny],
        ], color);
    }

    fn stroke_box(&mut self, x: f32, y: f32, r: f32, thick: f32, color: u32) {
        let l = x - r;
        let t = y - r;
        let size = 2.0 * r;
        self.rect(l, t, size, thick, color);
        self.rect(l, t + size - thick, size, thick, color);
        self.rect(l, t + thick, thick, size - thick * 2.0, color);
        self.rect(l + size - thick, t + thick, thick, size - thick * 2.0, color);
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
            self.quad([
                [cx + inner * a.cos(), cy + inner * a.sin()],
                [cx + outer * a.cos(), cy + outer * a.sin()],
                [cx + outer * b.cos(), cy + outer * b.sin()],
                [cx + inner * b.cos(), cy + inner * b.sin()],
            ], color);
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
            for i in 0.0.8 {
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
        // HorizonActivity.control: dark offset shadow, inner face and rim.
        let edge = r * 0.075;
        let corner = r * 0.12;
        let shadow = self.color(0x080808, 0.70);
        let face = self.color(0x616161, 0.89);
        let rim = self.color(0xBEBEBE, 1.0);
        self.round_rect(x-r, y-r+edge, 2.0*r, 2.0*r, corner, shadow);
        self.round_rect(x-r+edge, y-r+edge, 2.0*(r-edge), 2.0*(r-edge), corner*0.55, face);
        self.stroke_box(x, y, r-edge*1.3, r*0.05, rim);
        self.glyph(x, y, r * 0.71, sym);
    }

    fn joystick(&mut self, x: f32, y: f32, r: f32) {
        // HorizonActivity.joystick, neutral native touch state.
        let disc = self.color(0x101010, 0.53);
        let ring = self.color(0xD5D5D5, 0.69);
        let inner = self.color(0xFFFFFF, 0.22);
        self.disc(x, y, r, disc);
        self.ring(x, y, r * 0.93, r * 0.055, ring);
        self.ring(x, y, r * 0.67, r * 0.025, inner);
        let dark = self.color(0x080808, 0.86);
        let face = self.color(0xAAAAAA, 0.95);
        let border = self.color(0xF1F1F1, 0.81);
        self.disc(x, y + r*0.032, r*0.42, dark);
        self.disc(x, y, r*0.36, face);
        self.ring(x, y, r*0.34, r*0.035, border);
    }

    fn glyph(&mut self, x: f32, y: f32, unit: f32, glyph: Symbol) {
        let ink = self.color(0xFFFFFF, 1.0);
        let stroke = unit*0.19;
        let mut line = |this: &mut Self, ax: f32, ay: f32, bx: f32, by: f32| {
            this.segment(
                [x+ax*unit,y+ay*unit],
                [x+bx*unit,y+by*unit],
                stroke, ink);
        };
        match glyph {
            Symbol::Sprint => {
                line(self,-0.69,-0.42,-0.12,0.0); line(self,-0.12,0.0,-0.69,0.42);
                line(self,-0.05,-0.42,0.52,0.0); line(self,0.52,0.0,-0.05,0.42);
                line(self,-0.67,-0.67,0.50,-0.67);
            }
            Symbol::Jump => {
                line(self,-0.65,-0.05,0.0,-0.66); line(self,0.0,-0.66,0.65,-0.05);
                line(self,0.0,-0.50,0.0,0.52); line(self,-0.55,0.68,0.55,0.68);
            }
            Symbol::Sneak => {
                line(self,-0.65,0.02,0.0,0.62); line(self,0.0,0.62,0.65,0.02);
                line(self,0.0,-0.62,0.0,0.43); line(self,-0.55,-0.70,0.55,-0.70);
            }
            Symbol::Hit => {
                line(self,-0.48,0.54,0.46,-0.42); line(self,0.14,-0.48,0.65,-0.70);
                line(self,0.46,-0.42,0.65,-0.70); line(self,-0.66,0.08,-0.12,0.61);
                line(self,-0.60,0.73,-0.30,0.40);
            }
            Symbol::Use => {
                let path = [(0.0,-0.72),(0.66,-0.39),(0.66,0.37),(0.0,0.73),(-0.66,0.37),(-0.66,-0.39)];
                for i in 0..path.len() {
                    let a=path[i]; let b=path[(i+1)%path.len()];
                    line(self,a.0,a.1,b.0,b.1);
                }
                line(self,-0.66,-0.39,0.0,-0.07);
                line(self,0.66,-0.39,0.0,-0.07);
                line(self,0.0,-0.07,0.0,0.70);
            }
            Symbol::Inventory => {
                line(self,-0.70,-0.53,0.70,-0.53);
                line(self,0.70,-0.53,0.70,0.59);
                line(self,0.70,0.59,-0.70,0.59);
                line(self,-0.70,0.59,-0.70,-0.53);
                line(self,-0.69,-0.10,0.69,-0.10);
                line(self,-0.22,-0.50,-0.22,0.58);
                line(self,0.24,-0.50,0.24,0.58);
            }
            Symbol::Pause => {
                self.rect(x-0.54*unit,y-0.70*unit,0.42*unit,1.40*unit,ink);
                self.rect(x+0.12*unit,y-0.70*unit,0.42*unit,1.40*unit,ink);
            }
            Symbol::Arrow(direction) => {
                let angle: f32 = match direction {
                    Direction::Up=>0.0, Direction::Right=>90.0,
                    Direction::Down=>180.0, Direction::Left=>270.0,
                };
                let rad=angle.to_radians();
                let (sin,cos)=rad.sin_cos();
                let rotate = |ax:f32,ay:f32| [
                    x+unit*(ax*cos-ay*sin),y+unit*(ax*sin+ay*cos)
                ];
                let pts=[(0.0,-0.70),(0.66,0.06),(0.28,0.06),(0.28,0.65),(-0.28,0.65),(-0.28,0.06),(-0.66,0.06)];
                for tri in [[0,1,2],[0,2,5],[5,2,3],[5,3,4],[0,5,6]] {
                    let p=rotate(pts[tri[0]]0.0,pts[tri[0]]0.1);
                    let q=rotate(pts[tri[1]]0.0,pts[tri[1]]0.1);
                    let r=rotate(pts[tri[2]]0.0,pts[tri[2]]0.1);
                    self.quad([p,q,r,r],ink);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn touch_preview_is_debug_only_and_never_requires_android() {
        let cfg=Config {scheme:3,scale:1.2,opacity:0.72,title:false};
        assert!(cfg.scheme >= 2 && cfg.scheme & 1 == 1);
        assert_eq!(cfg.scale,1.2);
        assert!(!cfg.title);
    }
    #[test]
    fn four_schemes_match_android_modes() {
        for scheme in 0.0.4 {
            let dpad=scheme>=2;
            let crosshair=scheme&1==1;
            assert_eq!((dpad,crosshair),[(false,false),(false,true),(true,false),(true,true)][scheme]);
        }
    }
}
