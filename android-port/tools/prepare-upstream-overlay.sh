#!/usr/bin/env bash
# Prepare an Android build tree from pristine upstream Horizon plus this repo's
# Android-only overlay. Desktop source remains upstream-identical; all mobile
# hooks exist only in the disposable build tree.
set -euo pipefail

OVERLAY_ROOT="$(git rev-parse --show-toplevel)"
DEST="${1:-$OVERLAY_ROOT/.work/upstream-horizon}"
UPSTREAM_REF="${2:-main}"
UPSTREAM_URL="${HORIZON_UPSTREAM_URL:-https://github.com/horizon-rust/horizon.git}"

mkdir -p "$(dirname "$DEST")"
if [ ! -d "$DEST/.git" ]; then
  git clone --filter=blob:none "$UPSTREAM_URL" "$DEST"
fi

git -C "$DEST" fetch --prune origin "$UPSTREAM_REF"
git -C "$DEST" checkout --detach FETCH_HEAD
git -C "$DEST" reset --hard FETCH_HEAD
git -C "$DEST" clean -fdx
UPSTREAM_SHA="$(git -C "$DEST" rev-parse HEAD)"

echo "Preparing Android overlay on upstream $UPSTREAM_SHA"

# Android-owned files. Do not copy any ordinary upstream crate/source tree.
rm -rf "$DEST/android-port" "$DEST/crates/horizon-android"
cp -a "$OVERLAY_ROOT/android-port" "$DEST/android-port"
cp -a "$OVERLAY_ROOT/crates/horizon-android" "$DEST/crates/horizon-android"

# Mobile client modules are additive source owned by this overlay. Keeping them
# separate from the upstream checkout lets upstream freely refactor everything
# else while preserving our touch/instance UI.
mkdir -p "$DEST/crates/horizon-client/src/android" "$DEST/crates/horizon-client/src/game"
cp "$OVERLAY_ROOT/crates/horizon-client/src/android.rs" "$DEST/crates/horizon-client/src/android.rs"
cp -a "$OVERLAY_ROOT/crates/horizon-client/src/android/." "$DEST/crates/horizon-client/src/android/"
for file in touch.rs touch_icons.rs mobile_ui.rs touch_settings.rs; do
  cp "$OVERLAY_ROOT/crates/horizon-client/src/game/$file" "$DEST/crates/horizon-client/src/game/$file"
done

python3 - "$DEST" "$UPSTREAM_SHA" <<'PY'
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
upstream_sha = sys.argv[2]


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text()
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"prepare-upstream-overlay: {label}: anchor not found in {path}")
    path.write_text(text.replace(old, new, 1))


def insert_before_in_scope(text: str, scope_start: int, scope_end: int, needle: str, addition: str, label: str) -> str:
    at = text.find(needle, scope_start, scope_end)
    if at < 0:
        raise SystemExit(f"prepare-upstream-overlay: {label}")
    line = text.rfind("\n", scope_start, at) + 1
    return text[:line] + addition + text[line:]


# Add only the Android wrapper crate to the workspace. Everything else remains
# the exact upstream workspace at this SHA.
workspace = root / "Cargo.toml"
text = workspace.read_text()
if '"crates/horizon-android"' not in text:
    anchor = '    "crates/horizon-client",\n'
    if anchor not in text:
        raise SystemExit("prepare-upstream-overlay: workspace client member anchor missing")
    text = text.replace(anchor, anchor + '    "crates/horizon-android",\n', 1)
    workspace.write_text(text)

# NativeActivity is Android-only. Do not replace upstream's dependency graph;
# extend the current winit declaration and add JNI only for Android.
manifest = root / "crates/horizon-client/Cargo.toml"
text = manifest.read_text()
if 'features = ["android-native-activity"]' not in text:
    text, count = re.subn(
        r'^winit\s*=\s*"([^"]+)"\s*$',
        r'winit = { version = "\1", features = ["android-native-activity"] }',
        text,
        count=1,
        flags=re.M,
    )
    if count != 1:
        raise SystemExit("prepare-upstream-overlay: upstream winit declaration changed")
if "target_os = \"android\"" not in text:
    anchor = "[target.'cfg(windows)'.dependencies]\n"
    block = "[target.'cfg(target_os = \"android\")'.dependencies]\n# NativeActivity recreation / launcherless instance management.\njni = \"0.22\"\n\n"
    if anchor not in text:
        raise SystemExit("prepare-upstream-overlay: target dependency anchor missing")
    text = text.replace(anchor, block + anchor, 1)
manifest.write_text(text)

# Android exposes the current upstream game module through the library so the
# NativeActivity wrapper can run exactly the same game loop as desktop.
lib = root / "crates/horizon-client/src/lib.rs"
text = lib.read_text()
marker = 'pub mod android; // horizon-android overlay'
if marker not in text:
    anchor = '#![forbid(unsafe_code)]\n'
    addition = '''#![forbid(unsafe_code)]
#[cfg(target_os = "android")]
extern crate self as horizon_client;
#[cfg(target_os = "android")]
pub mod android; // horizon-android overlay
#[cfg(target_os = "android")]
pub mod game; // same upstream game loop, loaded by NativeActivity
'''
    if anchor not in text:
        raise SystemExit("prepare-upstream-overlay: lib.rs crate attribute anchor missing")
    text = text.replace(anchor, addition, 1)
    lib.write_text(text)

# Android-specific game modules are additive. They are not compiled on desktop.
game = root / "crates/horizon-client/src/game.rs"
text = game.read_text()
if 'mod touch; // horizon-android overlay' not in text:
    anchor = 'mod advancements;\n'
    addition = '''#[cfg(target_os = "android")]
mod touch; // horizon-android overlay
#[cfg(target_os = "android")]
mod touch_icons; // Horizon Android icon HUD
#[cfg(target_os = "android")]
mod mobile_ui; // Horizon Android launcherless manager
'''
    if anchor not in text:
        raise SystemExit("prepare-upstream-overlay: game module anchor missing")
    text = text.replace(anchor, addition + anchor, 1)

# Fresh upstream has no mobile touch dispatcher. Scope to Game's handler but do
# not depend on formatting/indentation of window_event: the only semantic anchor
# we require is follow_keymap inside Game::window_event.
if 'self.android_manager_key(event)' not in text:
    impl_marker = "impl ApplicationHandler for Game<'_> {"
    impl_start = text.find(impl_marker)
    if impl_start < 0:
        raise SystemExit("prepare-upstream-overlay: Game ApplicationHandler missing")
    next_impl = text.find("\nimpl ApplicationHandler", impl_start + len(impl_marker))
    impl_end = len(text) if next_impl < 0 else next_impl
    event_start = text.find("fn window_event", impl_start, impl_end)
    if event_start < 0:
        raise SystemExit("prepare-upstream-overlay: Game::window_event missing")
    follow_at = text.find("follow_keymap(&mut event);", event_start, impl_end)
    if follow_at < 0:
        raise SystemExit("prepare-upstream-overlay: Game::window_event follow_keymap hook changed")
    line = text.rfind("\n", event_start, follow_at) + 1
    indent = text[line:follow_at]
    hook = (
        f'{indent}#[cfg(target_os = "android")]\n'
        f'{indent}if let WindowEvent::KeyboardInput {{ event, .. }} = &event\n'
        f'{indent}    && self.android_manager_key(event)\n'
        f'{indent}{{\n'
        f'{indent}    return;\n'
        f'{indent}}}\n'
        f'{indent}#[cfg(target_os = "android")]\n'
        f'{indent}if let WindowEvent::Touch(touch) = &event {{\n'
        f'{indent}    self.touch(event_loop, id, touch);\n'
        f'{indent}    return;\n'
        f'{indent}}}\n'
    )
    text = text[:line] + hook + text[line:]

# NativeActivity has no meaningful process command line. Preserve upstream's
# parser unchanged on every non-Android target.
old = 'let Some(mut o) = Self::parse(std::env::args().skip(1))\n            .map_err(|e| launch_api::UsageError(e.to_string()))?'
new = '''let args = if cfg!(target_os = "android") {
            Vec::<String>::new().into_iter()
        } else {
            std::env::args().skip(1).collect::<Vec<_>>().into_iter()
        };
        let Some(mut o) = Self::parse(args)
            .map_err(|e| launch_api::UsageError(e.to_string()))?'''
if old in text:
    text = text.replace(old, new, 1)
elif new not in text:
    raise SystemExit("prepare-upstream-overlay: Options::read argv anchor changed")

# Android has no sibling executable resource directory. Prefer the explicit
# prepared-resource path supplied by the NativeActivity wrapper.
old = '''let sibling = std::env::current_exe()?
            .parent()
            .ok_or("missing executable directory")?
            .join("resources");
        let mut o = Self {
            resources: if sibling.join("horizon-resources.json").exists() {
                sibling
            } else {
                std::path::Path::new(".local/horizon-client").join(horizon_core::VANILLA_VERSION)
            },'''
new = '''#[cfg(not(target_os = "android"))]
        let sibling = std::env::current_exe()?
            .parent()
            .ok_or("missing executable directory")?
            .join("resources");
        #[cfg(target_os = "android")]
        let sibling = PathBuf::new();
        let mut o = Self {
            resources: std::env::var_os("HORIZON_RESOURCES")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    if sibling.join("horizon-resources.json").exists() {
                        sibling
                    } else {
                        std::path::Path::new(".local/horizon-client").join(horizon_core::VANILLA_VERSION)
                    }
                }),'''
if old in text:
    text = text.replace(old, new, 1)
elif 'std::env::var_os("HORIZON_RESOURCES")' not in text:
    raise SystemExit("prepare-upstream-overlay: resource path anchor changed")

# The wrapper persists the selected world in the instance folder.
world_marker = 'std::env::var("HORIZON_WORLD")'
if world_marker not in text:
    needle = '        Ok(Some(o))\n    }\n    /// The command line alone:'
    if needle not in text:
        raise SystemExit("prepare-upstream-overlay: Options::read return anchor changed")
    text = text.replace(
        needle,
        '''        #[cfg(target_os = "android")]
        if let Ok(world) = std::env::var("HORIZON_WORLD")
            && !world.trim().is_empty()
        {
            o.world = Some(world);
        }
        Ok(Some(o))
    }
    /// The command line alone:''',
        1,
    )

# Build winit's event loop from AndroidApp only on Android; desktop continues to
# call the same upstream EventLoop constructor through this tiny adapter.
if 'fn horizon_event_loop()' not in text:
    anchor = "fn run_event_loop(mut game: Game<'_>) -> Result<()> {"
    if anchor not in text:
        raise SystemExit("prepare-upstream-overlay: run_event_loop anchor changed")
    helper = '''#[cfg(target_os = "android")]
fn horizon_event_loop() -> Result<EventLoop<()>> {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    let app = horizon_client::android::app().ok_or("AndroidApp was not installed")?;
    Ok(EventLoop::builder().with_android_app(app).build()?)
}

#[cfg(not(target_os = "android"))]
fn horizon_event_loop() -> Result<EventLoop<()>> {
    Ok(EventLoop::new()?)
}

'''
    text = text.replace(anchor, helper + anchor, 1)
text = text.replace('EventLoop::new()?.run_app(&mut game)', 'horizon_event_loop()?.run_app(&mut game)')
text = text.replace('EventLoop::new()?.run_app(&mut app)', 'horizon_event_loop()?.run_app(&mut app)')

# Continuous movement is separate from button presses. Feed the phone stick into
# the same analog movement slot as a controller, preferring touch while held.
move_marker = 'let stick = self.touch_stick().or(stick);'
if move_marker not in text:
    anchor = '        let stick = self.pad_move();\n'
    if anchor not in text:
        raise SystemExit("prepare-upstream-overlay: pad movement anchor changed")
    text = text.replace(
        anchor,
        anchor + '        #[cfg(target_os = "android")]\n        let stick = self.touch_stick().or(stick);\n',
        1,
    )

# Android cannot rely on a desktop pointer grab. A live camera-touch gesture is
# itself a logical grab/focus source so MouseLook's accumulated deltas are
# actually consumed by the frame.
look_marker = 'let touch_pointer = self.touch_camera_active();'
if look_marker not in text:
    old = '''        let pointer = self.captured || self.agent_pointer;
        let grabbed = pointer && self.screen == Screen::Playing && self.player.is_some();
        let active = self.focused || self.agent_pointer;
'''
    new = '''        #[cfg(target_os = "android")]
        let touch_pointer = self.touch_camera_active();
        #[cfg(not(target_os = "android"))]
        let touch_pointer = false;
        let pointer = self.captured || self.agent_pointer || touch_pointer;
        let grabbed = pointer && self.screen == Screen::Playing && self.player.is_some();
        let active = self.focused || self.agent_pointer || touch_pointer;
'''
    if old not in text:
        raise SystemExit("prepare-upstream-overlay: mouse look gating anchor changed")
    text = text.replace(old, new, 1)

# The mobile controls must be rendered as part of the final HUD. Keep this at
# the end of HUD composition so controls sit above normal world HUD elements.
hud_marker = 'self.draw_touch_controls(&mut batch); // horizon-android overlay'
if hud_marker not in text:
    anchor = '        self.draw_pad_cursor(&mut batch);\n'
    if anchor not in text:
        raise SystemExit("prepare-upstream-overlay: HUD tail anchor changed")
    addition = '''        #[cfg(target_os = "android")]
        if self.screen == Screen::Playing && self.title.is_none() && !self.ui.hide_gui {
            self.draw_touch_controls(&mut batch); // horizon-android overlay
        }
'''
    text = text.replace(anchor, addition + anchor, 1)

game.write_text(text)

# Extend the additive touch module with the logical camera-grab query and make
# its hotbar hit target match the enlarged/elevated Android presentation.
touch = root / "crates/horizon-client/src/game/touch.rs"
t = touch.read_text()
t = t.replace('const HOTBAR_WIDTH: f32 = 182.0;', 'const HOTBAR_WIDTH: f32 = 228.0;')
t = t.replace('const HOTBAR_SLOT: f32 = 20.0;', 'const HOTBAR_SLOT: f32 = 25.0;')
t = t.replace('let y0 = f64::from(h - 25.0);', 'let y0 = f64::from(h - 58.0);')
if 'pub(super) fn touch_camera_active(&self)' not in t:
    anchor = '''    pub(super) fn touch_stick(&self) -> Option<[f32; 2]> {
        (self.touch_ui.stick != [0.0, 0.0]).then_some(self.touch_ui.stick)
    }
'''
    addition = anchor + '''
    pub(super) fn touch_camera_active(&self) -> bool {
        self.touch_ui
            .fingers
            .values()
            .any(|gesture| matches!(gesture, Gesture::Look { .. }))
    }
'''
    if anchor not in t:
        raise SystemExit("prepare-upstream-overlay: touch_stick method anchor changed")
    t = t.replace(anchor, addition, 1)
touch.write_text(t)

(root / ".horizon-upstream-sha").write_text(upstream_sha + "\n")
PY

# The existing patch stack now operates only on this disposable latest-upstream
# tree. If an upstream refactor invalidates an anchor, the build fails loudly
# rather than silently retaining stale Horizon code.
echo "$UPSTREAM_SHA" > "$DEST/android-port/UPSTREAM_SHA"
echo "Prepared $DEST from $UPSTREAM_SHA"
