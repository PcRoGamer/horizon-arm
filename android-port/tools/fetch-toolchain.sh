#!/usr/bin/env bash
# Installs what an Android build of Horizon needs, without root:
#   rustup + Android targets, a JDK, the Android SDK (build-tools, platform) and the NDK.
# Idempotent: everything already present is skipped. Lands in $REPO/.toolchain (gitignored).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TC="$REPO/.toolchain"
mkdir -p "$TC"

ANDROID_HOME="${ANDROID_HOME:-$TC/android-sdk}"
CMDLINE_URL="https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip"
JDK_API="https://api.adoptium.net/v3/binary/latest/21/ga/linux/x64/jdk/hotspot/normal/eclipse"
SDK_PACKAGES=(
  "platform-tools"
  "platforms;android-34"
  "build-tools;34.0.0"
  "ndk;27.2.12479018"
)
RUST_TARGETS=(x86_64-linux-android aarch64-linux-android)

log() { printf '\n=== %s\n' "$*"; }

# --- rustup + targets -------------------------------------------------------
if ! command -v rustup >/dev/null 2>&1 && [ ! -x "$TC/cargo/bin/rustup" ]; then
  log "rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --no-modify-path --profile default
fi
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export PATH="$CARGO_HOME/bin:$PATH"
for t in "${RUST_TARGETS[@]}"; do
  log "rust target $t"
  rustup target add "$t"
done

# --- JDK (needed only to run sdkmanager and apksigner) ----------------------
if [ ! -x "$TC/jdk/bin/java" ]; then
  log "JDK 21"
  mkdir -p "$TC/jdk"
  curl -L --fail --retry 5 --retry-delay 3 -o "$TC/jdk.tar.gz" "$JDK_API"
  rm -rf "$TC/jdk"; mkdir -p "$TC/jdk"
  tar -xzf "$TC/jdk.tar.gz" -C "$TC/jdk" --strip-components=1
  rm -f "$TC/jdk.tar.gz"
fi
export JAVA_HOME="$TC/jdk"
export PATH="$JAVA_HOME/bin:$PATH"

# --- Android SDK -----------------------------------------------------------
if [ ! -x "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" ]; then
  log "Android cmdline-tools"
  mkdir -p "$TC/dl" "$ANDROID_HOME/cmdline-tools"
  curl -L --fail --retry 5 --retry-delay 3 -o "$TC/dl/cmdline-tools.zip" "$CMDLINE_URL"
  unzip -q -o "$TC/dl/cmdline-tools.zip" -d "$TC/dl"
  rm -rf "$ANDROID_HOME/cmdline-tools/latest"
  mv "$TC/dl/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest"
fi
export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"

log "sdkmanager: ${SDK_PACKAGES[*]}"
yes | sdkmanager --sdk_root="$ANDROID_HOME" --licenses >/dev/null || true
sdkmanager --sdk_root="$ANDROID_HOME" "${SDK_PACKAGES[@]}"

log "done"
echo "ANDROID_HOME=$ANDROID_HOME"
ls "$ANDROID_HOME/ndk"
