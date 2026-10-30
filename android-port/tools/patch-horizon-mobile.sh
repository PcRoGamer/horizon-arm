#!/usr/bin/env bash
# Android-only source hooks for the launcherless mobile build.
#
# Keep Horizon's desktop sources upstream-shaped. The Android build applies a
# deliberately small set of hooks for the in-game manager, touch presentation,
# and Android SurfaceView lifecycle before compiling.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=tools/env.sh
source "$REPO/tools/env.sh"

python3 - "$HORIZON_SRC" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1])
game = root / "crates/horizon-client/src/game.rs"
screens = root / "crates/horizon-client/src/game/screens.rs"
# Upstream split the renderer out of horizon-client. Keep the Android surface
# lifecycle hook on the renderer's current home rather than the old gfx.rs path.
gfx = root / "crates/horizon-client-render/src/gfx.rs"
touch = root / "crates/horizon-client/src/game/touch.rs"


def replace_once(path: Path, old: str, new: str, marker: str) -> None:
    text = path.read_text()
    if marker in text:
        return
    if old not in text:
        raise SystemExit(f"patch-horizon-mobile: anchor not found in {path}: {old[:100]!r}")
    path.write_text(text.replace(old, new, 1))


def replace_between(path: Path, start: str, end: str, new: str, marker: str) -> None:
    text = path.read_text()
    if marker in text:
        return
    a = text.find(start)
    if a < 0:
        raise SystemExit(f"patch-horizon-mobile: start anchor not found in {path}: {start[:100]!r}")
    b = text.find(end, a)
    if b < 0:
        raise SystemExit(f"patch-horizon-mobile: end anchor not found in {path}: {end[:100]!r}")
    path.write_text(text[:a] + new + text[b:])


# Compile the Android-only UI modules next to the touch module.
replace_once(
    game,
    '#[cfg(target_os = "android")]\nmod touch;\n',
    '#[cfg(target_os = "android")]\nmod touch;\n#[cfg(target_os = "android")]\nmod touch_icons; // Horizon Android icon HUD\n#[cfg(target_os = "android")]\nmod mobile_ui; // Horizon Android launcherless manager\n',
    'mod touch_icons; // Horizon Android icon HUD',
)

# Name editing needs first refusal on keyboard events; otherwise the vanilla
# title/chat handlers would consume the same Enter/Backspace/text.
replace_once(
    game,
    '        #[cfg(target_os = "android")]\n        if let WindowEvent::Touch(touch) = &event {\n            self.touch(event_loop, id, touch);\n            return;\n        }\n',
    '        #[cfg(target_os = "android")]\n        if let WindowEvent::KeyboardInput { event, .. } = &event\n            && self.android_manager_key(event)\n        {\n            return;\n        }\n        #[cfg(target_os = "android")]\n        if let WindowEvent::Touch(touch) = &event {\n            self.touch(event_loop, id, touch);\n            return;\n        }\n',
    'self.android_manager_key(event)',
)

# The manager is a pause overlay, not a replacement pause screen.
replace_once(
    screens,
    '            ui::tooltip(batch, atlas, &lines, self.cursor);\n        }\n    }\n    pub(super) fn pause_click(&mut self) -> Result<()> {\n',
    '            ui::tooltip(batch, atlas, &lines, self.cursor);\n        }\n        #[cfg(target_os = "android")]\n        self.draw_android_pause_controls(batch);\n    }\n    pub(super) fn pause_click(&mut self) -> Result<()> {\n        #[cfg(target_os = "android")]\n        if self.android_pause_click() {\n            return Ok(());\n        }\n',
    'self.draw_android_pause_controls(batch)',
)

# Android destroys its SurfaceView when the Activity is suspended. Winit's
# contract requires every wgpu::Surface referring to it to be *dropped* before
# the Suspended callback returns. Renderer::release_surface intentionally keeps
# the old surface cached for desktop renderer replacement, so Android needs a
# separate hard detach while retaining all other GPU/chunk resources.
replace_once(
    gfx,
    '''    pub fn release_surface(&mut self) {
        *self
            .spare_surface
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = self.surface.take();
    }
''',
    '''    pub fn release_surface(&mut self) {
        *self
            .spare_surface
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = self.surface.take();
    }
    /// Android's SurfaceView has been destroyed. Drop every Surface that can
    /// still reference it, while retaining the device, pipelines and world GPU
    /// data for the next Activity resume.
    #[cfg(target_os = "android")]
    pub fn suspend_surface(&mut self) {
        self.surface = None;
        *self
            .spare_surface
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
    /// Attach the current Android SurfaceView after resume. Redundant Resumed
    /// events are harmless and keep the already-valid surface.
    #[cfg(target_os = "android")]
    pub fn resume_surface(&mut self) -> Result<()> {
        if self.offscreen.is_some() || self.surface.is_some() {
            return Ok(());
        }
        self.recreate_surface()
    }
''',
    'pub fn suspend_surface(&mut self)',
)

# The game keeps its Winit Window and all GPU resources across an Android app
# switch, but the swapchain surface itself must follow the SurfaceView lifecycle.
replace_once(
    game,
    '''impl ApplicationHandler for Game<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
''',
    '''impl ApplicationHandler for Game<'_> {
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "android")]
        {
            self.release_touch_inputs();
            self.set_soft_keyboard(false);
            self.keys.clear();
            self.physical_keys.clear();
            self.left = false;
            self.right = false;
            self.captured = false;
            self.focused = false;
            if let Some(renderer) = &mut self.renderer {
                renderer.suspend_surface();
            }
            self.previous = Instant::now();
            log::info!("Horizon Android: render surface suspended");
        }
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "android")]
        if self.window.is_some() {
            let size = self.window.as_ref().map(|w| w.inner_size()).unwrap_or_default();
            let result: Result<()> = (|| {
                let renderer = self.renderer.as_mut().ok_or("renderer missing on Android resume")?;
                renderer.resume_surface()?;
                if size.width > 0 && size.height > 0 {
                    renderer.resize(size.width, size.height);
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.error = Some(error);
                event_loop.exit();
                return;
            }
            self.previous = Instant::now();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            log::info!("Horizon Android: render surface resumed");
            return;
        }
        if self.window.is_some() {
            return;
        }
''',
    'Horizon Android: render surface suspended',
)

# The startup/loading renderer can be backgrounded too. It follows the same
# surface detach/reattach path instead of becoming permanently black.
replace_once(
    game,
    '''impl Loading {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
''',
    '''impl Loading {
    fn suspended(&mut self) {
        #[cfg(target_os = "android")]
        if let Some(renderer) = &mut self.renderer {
            renderer.suspend_surface();
            log::info!("Horizon Android: loading surface suspended");
        }
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "android")]
        if self.window.is_some() {
            let size = self.window.as_ref().map(|w| w.inner_size()).unwrap_or_default();
            let result: Result<()> = (|| {
                let renderer = self.renderer.as_mut().ok_or("loading renderer missing on Android resume")?;
                renderer.resume_surface()?;
                if size.width > 0 && size.height > 0 {
                    renderer.resize(size.width, size.height);
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.error = Some(error);
                event_loop.exit();
                return;
            }
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            log::info!("Horizon Android: loading surface resumed");
            return;
        }
        if self.window.is_some() {
            return;
        }
''',
    'Horizon Android: loading surface suspended',
)

# App owns whichever state is live, so it must forward the platform's suspend
# callback just as it already forwards resume.
replace_once(
    game,
    '''impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        match self {
            App::Loading(loading) => loading.resumed(event_loop),
            App::Ready(game) => game.resumed(event_loop),
        }
    }
''',
    '''impl ApplicationHandler for App {
    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        match self {
            App::Loading(loading) => loading.suspended(),
            App::Ready(game) => game.suspended(event_loop),
        }
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        match self {
            App::Loading(loading) => loading.resumed(event_loop),
            App::Ready(game) => game.resumed(event_loop),
        }
    }
''',
    'App::Loading(loading) => loading.suspended()',
)

# Parent lifecycle code needs to release touch state on suspend.
replace_once(
    touch,
    '    fn release_touch_inputs(&mut self) {\n',
    '    pub(super) fn release_touch_inputs(&mut self) {\n',
    'pub(super) fn release_touch_inputs',
)
replace_once(
    touch,
    '    fn set_soft_keyboard(&self, show: bool) {\n',
    '    pub(super) fn set_soft_keyboard(&self, show: bool) {\n',
    'pub(super) fn set_soft_keyboard',
)

# Replace desktop-looking text labels with a dedicated icon-first mobile HUD.
replace_once(
    touch,
    'use horizon_client::hud::{HudBatch, rgba};\n',
    'use horizon_client::hud::HudBatch;\nuse super::touch_icons;\n',
    'use super::touch_icons;',
)
replace_between(
    touch,
    '        batch.fill(atlas, sx - r, sy - r, r * 2.0, r * 2.0, rgba(8, 8, 12, 52));\n',
    '\n    }\n}\n\nfn look_at',
    '''        touch_icons::joystick(batch, atlas, sx, sy, r, self.touch_ui.stick);

        for (control, (cx, cy), r) in &layout.buttons {
            let down = self.touch_ui.pressed.contains(control);
            let icon = match control {
                Control::Jump => touch_icons::Icon::Jump,
                Control::Sneak => touch_icons::Icon::Sneak,
                Control::Sprint => touch_icons::Icon::Sprint,
                Control::Attack => touch_icons::Icon::Attack,
                Control::Use => touch_icons::Icon::Use,
                Control::Inventory => touch_icons::Icon::Inventory,
                Control::Chat => touch_icons::Icon::Chat,
                Control::Pause => touch_icons::Icon::Pause,
            };
            touch_icons::button(batch, atlas, icon, *cx, *cy, *r, down);
        }
''',
    'touch_icons::joystick(batch, atlas, sx, sy, r, self.touch_ui.stick);',
)

print("patched Horizon Android lifecycle, icon HUD and launcherless manager hooks")
PY
