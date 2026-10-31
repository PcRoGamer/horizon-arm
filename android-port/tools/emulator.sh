#!/usr/bin/env bash
# An Android emulator for trying the port on this machine: x86_64, Android 14,
# KVM. Creates the AVD once, then boots it and waits until it is up.
#
#   tools/emulator.sh              headless (for scripts and screenshots)
#   tools/emulator.sh --window     a window you can play in
#   tools/emulator.sh --software   SwiftShader instead of the host GPU (no Vulkan)
#
# The host GPU is the default: gfxstream hands the guest Vulkan to this
# machine's driver, which is what a phone with a Vulkan driver looks like to
# wgpu. SwiftShader has no Vulkan, so the game's GL fallback is what runs there.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=tools/env.sh
source "$REPO/tools/env.sh"

AVD=horizon34
IMAGE="system-images;android-34;google_apis;x86_64"
GPU=host
WINDOW=(-no-window)

for arg in "$@"; do
  case "$arg" in
    --window) WINDOW=() ;;
    --software) GPU=swiftshader_indirect ;;
    *) echo "usage: $0 [--window] [--software]" >&2; exit 2 ;;
  esac
done

if ! emulator -list-avds | grep -qx "$AVD"; then
  echo "=== creating $AVD ($IMAGE)"
  [ -d "$ANDROID_HOME/system-images/android-34/google_apis/x86_64" ] || {
    echo "missing $IMAGE; run: sdkmanager \"$IMAGE\" \"emulator\"" >&2
    exit 1
  }
  echo no | avdmanager create avd -n "$AVD" -k "$IMAGE" -d pixel_6 --force
fi

# The host keyboard is the phone's hardware keyboard: the port reads it for
# movement, so the AVD must not claim the device has none (avdmanager's
# default). The change applies on the next boot.
CONFIG="$HOME/.android/avd/$AVD.avd/config.ini"
if [ -f "$CONFIG" ]; then
  if grep -q '^hw\.keyboard' "$CONFIG"; then
    sed -i -E 's/^hw\.keyboard *=.*/hw.keyboard = yes/' "$CONFIG"
  else
    printf 'hw.keyboard = yes\n' >> "$CONFIG"
  fi
fi

echo "=== booting $AVD (gpu $GPU)"
emulator -avd "$AVD" "${WINDOW[@]}" -no-audio -no-boot-anim -no-snapshot \
  -memory "${HORIZON_EMULATOR_MEMORY:-3072}" -cores "${HORIZON_EMULATOR_CORES:-4}" -gpu "$GPU" -netdelay none -netspeed full &

adb wait-for-device
for _ in $(seq 1 120); do
  [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && break
  sleep 2
done
echo "=== up: $(adb shell getprop ro.build.version.release | tr -d '\r') ($(adb shell getprop ro.product.cpu.abi | tr -d '\r'))"
