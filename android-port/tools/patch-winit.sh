#!/usr/bin/env bash
# Patches winit's Android backend so a controller and the Back button reach the
# game.
#
#   tools/patch-winit.sh
#
# winit has no gamepad API. Its Android backend turns every motion event into a
# touch and drops the key codes it has no `KeyCode` for, so a pad's sticks are
# phantom touches and its buttons vanish. The patch here sends both out as
# `DeviceEvent`s (Motion, Button) instead; horizon-client reads them in
# crates/horizon-client/src/game/pad/android.rs.
#
# Android's Back button also has no `KeyCode` winit knows: it is only a logical
# `BrowserBack`, and the game reads physical keys, so no screen would ever close
# with it. The second patch makes Back arrive as `Escape`.
#
# Both are applied to the copy in the cargo registry, not to a vendored fork, so
# the upstream tree stays exactly as patches/ made it. Cargo trusts a registry
# package's checksum rather than its files, so the first build after patching
# needs winit rebuilt: this script drops its artifacts.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(sed -n 's/^version: *//p' "$REPO/winit/winit.txt")"

CRATE="$(ls -d "$HOME"/.cargo/registry/src/*/"winit-$VERSION" 2>/dev/null | head -1 || true)"
if [ -z "$CRATE" ]; then
  echo "patch-winit: winit $VERSION is not in the cargo registry yet; build once (cargo fetch) first" >&2
  exit 1
fi

# patch file, the marker that says it is already applied, patched file, the
# line the marker sits on
PATCHES=(
  "$REPO/winit/winit-$VERSION-android-gamepad.patch|Horizon: a controller is not a pointer|src/platform_impl/android/mod.rs"
  "$REPO/winit/winit-$VERSION-android-back.patch|Horizon: Android's Back button is the screen-closing key|src/platform_impl/android/keycodes.rs"
)

patched=0
for entry in "${PATCHES[@]}"; do
  IFS='|' read -r patch mark file <<<"$entry"
  if grep -q "$mark" "$CRATE/$file"; then
    continue
  fi
  echo "=== patching winit $VERSION: $(basename "$patch")"
  patch -p1 -s -d "$CRATE" < "$patch"
  patched=1
done

if [ "$patched" = 0 ]; then
  exit 0
fi

# The patched source has to be compiled again; the checksum would otherwise
# say the crate is up to date.
# shellcheck source=tools/env.sh
source "$REPO/tools/env.sh" >/dev/null 2>&1 || true
( cd "${HORIZON_SRC:-$REPO/..}" && cargo clean -p winit >/dev/null 2>&1 || true )
echo "patched; winit will be rebuilt"
