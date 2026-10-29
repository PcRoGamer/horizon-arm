#!/usr/bin/env bash
# Copies prepared resources (horizon-prepare's output, a folder with
# horizon-resources.json) into the app's own files folder, where the port looks
# for them. Needs a root adb shell: the emulator's `google_apis` images and
# Waydroid both have one.
#
#   tools/push-resources.sh [folder]
#
# Default folder: ../horizon/.local/horizon-client/26.3 (where horizon-prepare
# writes when run from the checkout).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=tools/env.sh
source "$REPO/tools/env.sh"

SRC="${1:-$HORIZON_SRC/.local/horizon-client/26.3}"
[ -f "$SRC/horizon-resources.json" ] || {
  echo "$SRC is not a prepared folder (no horizon-resources.json)" >&2
  echo "run: cargo run --release -p horizon-prepare -- --prepare --server-jar <server.jar>" >&2
  exit 1
}

APP=dev.horizon.android
DATA="/data/user/0/$APP/files"
TARGET="$DATA/.local/horizon-client/$(basename "$SRC")"

adb root >/dev/null 2>&1 || true
adb wait-for-device
if [ "$(adb shell id -u | tr -d '\r')" != 0 ]; then
  echo "no root shell: this script needs one (an emulator image or Waydroid)" >&2
  exit 1
fi

echo "=== pushing $(du -sh "$SRC" | cut -f1) to $TARGET"
adb shell am force-stop "$APP" || true
adb shell mkdir -p "$TARGET"
adb push "$SRC/." "$TARGET/" | tail -1

# The app's own uid, so it can write its log and saves there.
APPUID="$(adb shell cmd package list packages -U | grep "package:$APP " | sed 's/.*uid://' | tr -d '\r')"
[ -n "$APPUID" ] || { echo "could not find $APP's uid; install the APK first" >&2; exit 1; }
adb shell chown -R "$APPUID:$APPUID" "$DATA"

echo "=== done: $TARGET"
