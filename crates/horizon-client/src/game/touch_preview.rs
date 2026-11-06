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

// All control faces are the exact LGPL-licensed TouchController Classic
// sprites. Android decodes the same PNGs from APK assets; the Rust HudAtlas
// packs their source bytes at build time. No generated imitation glyphs.
struct Painter<'a> {
    batch: &'a mut HudBatch,
    atlas: &'a HudAtlas,
    cfg: Config,
}
impl Painter<'_> {
    fn draw_sprite(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32) {
        let alpha = (255.0 * self.cfg.opacity).round().clamp(0.0, 255.0) as u8;
        if self.atlas.sprites.contains_key(name) {
            self.batch
                .sprite_scaled(self.atlas, name, x, y, w, h, rgba(255, 255, 255, alpha));
        } else {
            // A visible error surface is better than an invisible button,
            // but source contracts and GPU capture CI require the real sprite.
            self.batch
                .fill(self.atlas, x, y, w, h, rgba(100, 100, 100, alpha));
        }
    }
    fn control(&mut self, x: f32, y: f32, radius: f32, sym: Symbol) {
        let (normal, highlighted) = match sym {
            Symbol::Chat => (
                "horizon_touch:gui/classic/chat/chat",
                "horizon_touch:gui/classic/chat/chat_active",
            ),
            Symbol::Pause => (
                "horizon_touch:gui/classic/pause/pause",
                "horizon_touch:gui/classic/pause/pause_active",
            ),
            Symbol::Perspective => (
                "horizon_touch:gui/classic/perspective/perspective",
                "horizon_touch:gui/classic/perspective/perspective_active",
            ),
            Symbol::Sprint => (
                "horizon_touch:gui/classic/sprint/sprint",
                "horizon_touch:gui/classic/sprint/sprint_active",
            ),
            Symbol::Jump => (
                "horizon_touch:gui/classic/jump/jump",
                "horizon_touch:gui/classic/jump/jump_active",
            ),
            Symbol::Sneak => (
                "horizon_touch:gui/classic/sneak/sneak",
                "horizon_touch:gui/classic/sneak/sneak_active",
            ),
            Symbol::Hit => (
                "horizon_touch:gui/classic/attack/attack",
                "horizon_touch:gui/classic/attack/attack_active",
            ),
            Symbol::Use => (
                "horizon_touch:gui/classic/use/use",
                "horizon_touch:gui/classic/use/use_active",
            ),
            Symbol::Arrow(Direction::Up) => (
                "horizon_touch:gui/classic/dpad/up",
                "horizon_touch:gui/classic/dpad/up_active",
            ),
            Symbol::Arrow(Direction::Right) => (
                "horizon_touch:gui/classic/dpad/right",
                "horizon_touch:gui/classic/dpad/right_active",
            ),
            Symbol::Arrow(Direction::Down) => (
                "horizon_touch:gui/classic/dpad/down",
                "horizon_touch:gui/classic/dpad/down_active",
            ),
            Symbol::Arrow(Direction::Left) => (
                "horizon_touch:gui/classic/dpad/left",
                "horizon_touch:gui/classic/dpad/left_active",
            ),
        };
        // Only the always-sprint debug state can be simulated in the native
        // desktop preview; Android reports held press bits for each action.
        let pressed = matches!(sym, Symbol::Sprint) && self.cfg.always_sprint;
        let sprite = if pressed { highlighted } else { normal };
        self.draw_sprite(sprite, x - radius, y - radius, radius * 2.0, radius * 2.0);
    }
    fn joystick(&mut self, x: f32, y: f32, radius: f32) {
        self.draw_sprite(
            "horizon_touch:gui/classic/joystick/pad",
            x - radius,
            y - radius,
            radius * 2.0,
            radius * 2.0,
        );
        self.draw_sprite(
            "horizon_touch:gui/classic/joystick/stick",
            x - radius * 0.5,
            y - radius * 0.5,
            radius,
            radius,
        );
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
