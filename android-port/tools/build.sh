#!/usr/bin/env bash
# Builds the APK: the game, cross-compiled for the ABIs asked, packaged with
# aapt2 and signed with the local debug key.
#
#   tools/build.sh [x86_64|arm64|both] [profile]
#
# x86_64 — Waydroid, emulators, x86 Chromebooks (default).
# arm64  — phones, and Waydroid images with libhybris on ARM hosts.
# Profile defaults to `fast` (optimised, quick to link); `release` is the
# shipping build, much slower to produce.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=tools/env.sh
source "$REPO/tools/env.sh"

ABI_ARG="${1:-x86_64}"
PROFILE="${2:-fast}"
VERSION_NAME="$(grep -m1 '^version' "$HORIZON_SRC/crates/horizon-android/Cargo.toml" | cut -d'"' -f2)"
VERSION_NAME="${VERSION_NAME:-0.1.0}"
OUT="$REPO/build"
mkdir -p "$OUT"

# winit's Android backend needs the controller patch before anything compiles
# (idempotent; it also drops the crate's artifacts the first time).
"$REPO/tools/patch-winit.sh"

# arboard has no Android backend. Keep Horizon's normal clipboard untouched on
# desktop and retain its process-local copy/paste fallback in the Android tree.
python3 "$REPO/tools/patch-android-clipboard.py" "$HORIZON_SRC"
# Keep desktop gilrs out of Android while preserving Horizon's controller
# semantics through the Android DeviceEvent -> PadState adapter.
python3 "$REPO/tools/patch-android-gamepad.py" "$HORIZON_SRC"
# The touch module is additive too; upstream Game only receives its two
# Android-gated state fields in the disposable APK build tree.
python3 "$REPO/tools/patch-android-touch-state.py" "$HORIZON_SRC"

# The current touch source already owns its icon HUD. Older Android hook code
# recognises standalone markers, so normalize the grouped import and add the
# legacy icon-patch marker before running that idempotent patcher. This changes
# only the CI/build worktree; the real Bedrock HUD stays in touch.rs/icons.rs.
TOUCH_RS="$HORIZON_SRC/crates/horizon-client/src/game/touch.rs"
python3 - "$TOUCH_RS" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
text = text.replace(
    'use super::{Game, Screen, touch_icons};',
    'use super::{Game, Screen};\nuse super::touch_icons;',
    1,
)
legacy = 'touch_icons::joystick(batch, atlas, sx, sy, r, self.touch_ui.stick);'
if legacy not in text:
    text = text.replace(
        'use super::touch_icons;',
        'use super::touch_icons;\n// Legacy Android hook marker: ' + legacy,
        1,
    )
path.write_text(text)
PY

# Keep Android's launcherless manager as a small set of explicit game hooks
# instead of carrying a divergent copy of Horizon's large game loop files.
bash "$REPO/tools/patch-horizon-mobile.sh"
# Bedrock-style touch options live in Horizon's pause UI and apply to the
# current instance without exposing a launcher/config-file workflow.
python3 "$REPO/tools/patch-touch-settings.py" "$HORIZON_SRC"
# Legacy parity patchers predate the live layout editor and used global anchors.
# Normalize the resulting source so hotbar-only wrapping stays entirely inside
# the runtime draw_touch_controls method instead of crossing Game impl methods.
python3 "$REPO/tools/repair-touch-overlay.py" "$HORIZON_SRC"
# Bedrock/Pocket presentation is Android-only: preserve upstream Java HUD parity
# in Git and move only the APK's anchors/crosshair/hotbar presentation.
python3 "$REPO/tools/patch-bedrock-hud.py" "$HORIZON_SRC"

case "$ABI_ARG" in
  x86_64) ABIS=(x86_64) ;;
  arm64) ABIS=(arm64) ;;
  both) ABIS=(x86_64 arm64) ;;
  *) echo "usage: $0 [x86_64|arm64|both] [profile]" >&2; exit 2 ;;
esac

target_for() { case "$1" in x86_64) echo x86_64-linux-android ;; arm64) echo aarch64-linux-android ;; esac; }
abi_dir() { case "$1" in x86_64) echo x86_64 ;; arm64) echo arm64-v8a ;; esac; }

STAGE="$OUT/stage"
rm -rf "$STAGE"; mkdir -p "$STAGE/lib"

for abi in "${ABIS[@]}"; do
  target="$(target_for "$abi")"
  echo "=== building $target ($PROFILE)"
  ( cd "$HORIZON_SRC" && cargo build -j "${HORIZON_JOBS:-4}" --target "$target" --profile "$PROFILE" -p horizon-android )
  so="$HORIZON_SRC/target/$target/$PROFILE/libhorizon_android.so"
  [ -f "$so" ] || { echo "missing $so" >&2; exit 1; }
  mkdir -p "$STAGE/lib/$(abi_dir "$abi")"
  out="$STAGE/lib/$(abi_dir "$abi")/libhorizon_android.so"
  cp "$so" "$out"
  "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip" \
    --strip-unneeded "$out"
  echo "    $(du -h "$so" | cut -f1) → $(du -h "$out" | cut -f1)  $out"
done

echo "=== packaging"
AAPT2="$ANDROID_HOME/build-tools/34.0.0/aapt2"
ZIPALIGN="$ANDROID_HOME/build-tools/34.0.0/zipalign"
APKSIGNER="$ANDROID_HOME/build-tools/34.0.0/apksigner"
JAR="$ANDROID_HOME/platforms/android-34/android.jar"

# One aapt asset directory holds either a development full resource tree, the
# redistributable Horizon-only facts used by first-run bootstrap, or both.
ASSET_DIR="$OUT/assets"
rm -rf "$ASSET_DIR"; mkdir -p "$ASSET_DIR"
HAS_ASSETS=0

# A full APK is still useful for local development with player-supplied data.
if [ -z "${HORIZON_NO_ASSETS:-}" ]; then
  PREPARED="${HORIZON_PREPARED:-$HORIZON_SRC/.local/horizon-client/26.3}"
  [ -f "$PREPARED/horizon-resources.json" ] || {
    echo "no prepared resources at $PREPARED" >&2
    echo "run horizon-prepare first, or set HORIZON_NO_ASSETS=1 for bootstrap/thin APK" >&2
    exit 1
  }
  VERSION_DIR="$(basename "$PREPARED")"
  mkdir -p "$ASSET_DIR/resources"
  echo "=== resources: $VERSION_DIR ($(du -sh "$PREPARED" | cut -f1))"
  cp -al "$PREPARED" "$ASSET_DIR/resources/$VERSION_DIR" 2>/dev/null ||
    cp -r "$PREPARED" "$ASSET_DIR/resources/$VERSION_DIR"
  ( cd "$ASSET_DIR" && find "resources/$VERSION_DIR" -type f ! -name ".*" | LC_ALL=C sort > resources.list )
  echo "    $(wc -l < "$ASSET_DIR/resources.list") files"
  HAS_ASSETS=1
fi

# The distributable Android build carries no Mojang files. It can instead carry
# a small archive of Horizon-computed tables; the phone downloads the pinned
# client/server jars directly from Mojang and combines them on first launch.
if [ -n "${HORIZON_FACTS_ZIP:-}" ]; then
  [ -f "$HORIZON_FACTS_ZIP" ] || { echo "missing HORIZON_FACTS_ZIP=$HORIZON_FACTS_ZIP" >&2; exit 1; }
  cp "$HORIZON_FACTS_ZIP" "$ASSET_DIR/horizon-facts-26.3.zip"
  echo "=== bootstrap facts: $(du -h "$HORIZON_FACTS_ZIP" | cut -f1)"
  HAS_ASSETS=1
fi

ASSETS=()
if [ "$HAS_ASSETS" -eq 1 ]; then
  ASSETS=(-A "$ASSET_DIR")
fi

"$AAPT2" link \
  -o "$OUT/unaligned.apk" \
  -I "$JAR" \
  --manifest "$REPO/AndroidManifest.xml" \
  --min-sdk-version 26 \
  --target-sdk-version 34 \
  --version-code 1 \
  --version-name "$VERSION_NAME" \
  "${ASSETS[@]}"

( cd "$STAGE" && zip -q -r "$OUT/unaligned.apk" lib )

"$ZIPALIGN" -p -f 4 "$OUT/unaligned.apk" "$OUT/aligned.apk"

KEYSTORE="$REPO/build/horizon-debug.jks"
if [ ! -f "$KEYSTORE" ]; then
  echo "=== debug keystore"
  keytool -genkeypair -keystore "$KEYSTORE" -storepass horizon -keypass horizon \
    -alias horizon -keyalg RSA -keysize 2048 -validity 10000 \
    -dname "CN=Horizon Android, O=Horizon, C=FR" >/dev/null 2>&1
fi

APK="$OUT/horizon-android-$ABI_ARG.apk"
"$APKSIGNER" sign --ks "$KEYSTORE" --ks-pass pass:horizon --key-pass pass:horizon \
  --out "$APK" "$OUT/aligned.apk"
"$APKSIGNER" verify --print-certs "$APK" | head -4

rm -f "$OUT/unaligned.apk" "$OUT/aligned.apk"
echo
echo "APK: $APK  ($(du -h "$APK" | cut -f1))"
echo
echo "Install (Waydroid):"
echo "  waydroid app install $APK"
echo "  waydroid shell -- logcat -s Horizon:V AndroidRuntime:E"
