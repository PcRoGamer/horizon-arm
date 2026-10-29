#!/usr/bin/env bash
# Installs the APK, starts it and follows what it says.
#
#   tools/run.sh [x86_64|arm64]     (default x86_64)
#
# The game writes its own log to the app's files (`logs/latest.log`); this
# prints that too, a few seconds after the start.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=tools/env.sh
source "$REPO/tools/env.sh"

ABI="${1:-x86_64}"
APK="$REPO/build/horizon-android-$ABI.apk"
[ -f "$APK" ] || { echo "no $APK; run tools/build.sh $ABI fast" >&2; exit 1; }

APP=dev.horizon.android
DATA="/data/user/0/$APP/files"

echo "=== install $APK"
adb install -r "$APK" | tail -1
adb logcat -c
adb shell am start -n "$APP/android.app.NativeActivity" | tail -1

# The first run of an APK that carries the data unpacks ~320 MB before the
# game starts; wait for that to be over (or a minute) instead of a fixed nap.
for _ in $(seq 1 60); do
  if adb logcat -d -s Horizon:V | grep -q "Horizon: resources\|no prepared resources"; then
    break
  fi
  sleep 2
done
sleep 6

echo "=== logcat"
adb logcat -d -s Horizon:V AndroidRuntime:E | tail -30
echo
echo "=== the game's log (last 20 lines)"
adb shell "tail -20 $DATA/logs/latest.log 2>/dev/null" || true
echo
echo "=== follow: adb logcat -s Horizon:V"
echo "=== screen: adb exec-out screencap -p > screen.png"
