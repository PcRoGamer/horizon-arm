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
for file in touch.rs touch_icons.rs mobile_ui.rs; do
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

# The normal desktop binary still owns game.rs. Android also exposes that same
# current-upstream game module through the library so NativeActivity can run it.
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
mod touch_icons; // horizon-android overlay
#[cfg(target_os = "android")]
mod mobile_ui; // horizon-android overlay
'''
    if anchor not in text:
        raise SystemExit("prepare-upstream-overlay: game module anchor missing")
    text = text.replace(anchor, addition + anchor, 1)

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
if old not in text:
    raise SystemExit("prepare-upstream-overlay: Options::read argv anchor changed")
text = text.replace(old, new, 1)

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
if old not in text:
    raise SystemExit("prepare-upstream-overlay: resource path anchor changed")
text = text.replace(old, new, 1)

# The wrapper persists the selected world in the instance folder.
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
    anchor = 'fn run_event_loop(mut game: Game<\'_>) -> Result<()> {'
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
game.write_text(text)

(root / ".horizon-upstream-sha").write_text(upstream_sha + "\n")
PY

# The existing patch stack now operates only on this disposable latest-upstream
# tree. If an upstream refactor invalidates an anchor, the build fails loudly
# rather than silently retaining stale Horizon code.
echo "$UPSTREAM_SHA" > "$DEST/android-port/UPSTREAM_SHA"
echo "Prepared $DEST from $UPSTREAM_SHA"
