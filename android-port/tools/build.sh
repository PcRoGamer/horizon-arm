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
  # Capped: a few of these crates are big, and rustc at every core next to a
  # running emulator (4 GB) is how a 16 GB machine runs out.
  ( cd "$HORIZON_SRC" && cargo build -j "${HORIZON_JOBS:-4}" --target "$target" --profile "$PROFILE" -p horizon-android )
  so="$HORIZON_SRC/target/$target/$PROFILE/libhorizon_android.so"
  [ -f "$so" ] || { echo "missing $so" >&2; exit 1; }
  mkdir -p "$STAGE/lib/$(abi_dir "$abi")"
  # The symbol table is worthless in an APK and is a fifth of the library:
  # rustc leaves the symbols in, the NDK's strip takes them out (a stripped
  # `fast` build is 41 MB instead of 58 MB, 18 MB packed instead of 20).
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

# The game's data goes inside the APK unless HORIZON_NO_ASSETS is set: a phone
# installs one file, and `crates/horizon-android/src/bundled.rs` unpacks it on
# the first run. The files are hard links, so staging costs no disk.
# `HORIZON_PREPARED` points at a prepared folder other than this checkout's.
ASSETS=()
if [ -z "${HORIZON_NO_ASSETS:-}" ]; then
  PREPARED="${HORIZON_PREPARED:-$HORIZON_SRC/.local/horizon-client/26.3}"
  [ -f "$PREPARED/horizon-resources.json" ] || {
    echo "no prepared resources at $PREPARED" >&2
    echo "run horizon-prepare first, or set HORIZON_NO_ASSETS=1 for a thin APK" >&2
    exit 1
  }
  VERSION_DIR="$(basename "$PREPARED")"
  ASSET_DIR="$OUT/assets"
  rm -rf "$ASSET_DIR"; mkdir -p "$ASSET_DIR/resources"
  echo "=== resources: $VERSION_DIR ($(du -sh "$PREPARED" | cut -f1))"
  cp -al "$PREPARED" "$ASSET_DIR/resources/$VERSION_DIR" 2>/dev/null ||
    cp -r "$PREPARED" "$ASSET_DIR/resources/$VERSION_DIR"
  # `AAssetDir` lists files only, so the unpacker is given the list (sorted,
  # so an unchanged pack produces an unchanged APK). `aapt2` drops hidden
  # files (`assets/.mcassetsroot` and its like, which the game never reads),
  # so they are left out of the list too.
  ( cd "$ASSET_DIR" && find "resources/$VERSION_DIR" -type f ! -name ".*" | LC_ALL=C sort > resources.list )
  echo "    $(wc -l < "$ASSET_DIR/resources.list") files"
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
