#!/usr/bin/env bash
# Actual native x86_64 Android game smoke, not ARM64-via-translation emulation.
# Run under reactivecircus/android-emulator-runner's script entry point.
set -euo pipefail
out=.local/android/x86-smoke
mkdir -p "$out"
collect() {
  adb logcat -d -v time > "$out/logcat.txt" 2>/dev/null || true
  adb shell dumpsys activity activities > "$out/activity.txt" 2>/dev/null || true
  adb exec-out screencap -p > "$out/horizon-x86.png" 2>/dev/null || true
}
trap collect EXIT
adb install -r .local/android/horizon-x86-debug.apk
adb shell settings put system accelerometer_rotation 0
adb shell settings put system user_rotation 1
adb shell getprop ro.product.cpu.abilist > "$out/guest-abis.txt"
adb shell getprop ro.dalvik.vm.native.bridge > "$out/native-bridge.txt"
adb shell getprop net.dns1 > "$out/dns-prop.txt"
# A failure here is diagnostic only: some emulator images omit ping.
adb shell ping -c 1 -W 2 piston-meta.mojang.com > "$out/dns-probe.txt" 2>&1 || true
adb logcat -c
adb shell am start -n dev.horizon.android/.HorizonActivity
ready=0
rendered=0
# First installation downloads verified Minecraft assets, then boots the
# native renderer. An eight-second "pidof" check is a false-positive test.
for ((probe=0; probe<70; probe++)); do
  sleep 6
  adb logcat -d -v time > "$out/logcat.txt"
  if grep -q 'HorizonNative STAGE 07: Mojang resources prepared' "$out/logcat.txt"; then
    ready=1
  fi
  if grep -iq 'first frame after' "$out/logcat.txt"; then
    rendered=1
  fi
  if [[ "$ready" -eq 1 && "$rendered" -eq 1 ]]; then break; fi
  if grep -q 'Minecraft resource bootstrap failed:' "$out/logcat.txt"; then
    echo 'FAILED: Mojang bootstrap; inspect logcat.txt and dns-probe.txt' >&2
    grep -E 'HorizonNative STAGE|Minecraft resource bootstrap failed|Dns Failed' "$out/logcat.txt" | tail -18 >&2 || true
    exit 1
  fi
  if ! adb shell pidof dev.horizon.android >/dev/null 2>&1; then
    echo 'FAILED: Android game exited before bootstrap and first frame' >&2
    grep -E 'HorizonNative STAGE|RustPanic|Fatal signal|FATAL EXCEPTION' "$out/logcat.txt" | tail -30 >&2 || true
    exit 1
  fi
done
if [[ "$ready" -ne 1 || "$rendered" -ne 1 ]]; then
  echo "FAILED: resource readiness=$ready, first GPU frame=$rendered after 420 seconds" >&2
  grep -E 'HorizonNative STAGE|Mojang resources:|first frame after' "$out/logcat.txt" | tail -30 >&2 || true
  exit 1
fi
collect
# Exercise the Java profile bridge on the real game's title screen.
adb shell input tap 2220 105
sleep 4
adb shell dumpsys activity activities > "$out/profile-activity.txt"
adb exec-out screencap -p > "$out/horizon-profile.png" || true
if ! grep -q 'ProfileActivity' "$out/profile-activity.txt"; then
  echo 'FAILED: profile navigation was not reached' >&2
  exit 1
fi
echo 'NATIVE_X86_ANDROID_BOOTSTRAP_RENDER_AND_PROFILE_OK'
