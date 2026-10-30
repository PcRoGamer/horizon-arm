#!/usr/bin/env python3
"""Inject Android controller input without enabling gilrs.

Upstream Horizon's controller stack uses gilrs on desktop. gilrs has no Android
backend, so the APK keeps the upstream controller semantics/settings while raw
Android pad events are translated into the same PadState in an additive module.
The Game hook is intentionally tied only to the stable ApplicationHandler
method signature so unrelated upstream mouse/focus refactors do not break it.
"""
from pathlib import Path
import shutil
import subprocess
import sys

root = Path(sys.argv[1])
port = Path(__file__).resolve().parents[1]
pad = root / "crates/horizon-client/src/game/pad.rs"
game = root / "crates/horizon-client/src/game.rs"
dest = root / "crates/horizon-client/src/game/pad/android.rs"
src = port / "overlay/game/pad/android.rs"
dest.parent.mkdir(parents=True, exist_ok=True)
shutil.copy2(src, dest)


def replace_once(path: Path, old: str, new: str, marker: str) -> None:
    text = path.read_text()
    if marker in text:
        return
    if old not in text:
        raise SystemExit(f"patch-android-gamepad: anchor not found in {path}: {old[:120]!r}")
    path.write_text(text.replace(old, new, 1))


replace_once(
    pad,
    '''    #[cfg(feature = "controllers")]
    devices: hardware::Devices,
    /// The virtual pad of `horizon-ctl pad`.
''',
    '''    #[cfg(feature = "controllers")]
    devices: hardware::Devices,
    /// Android pads arrive through the patched winit backend, not gilrs.
    #[cfg(target_os = "android")]
    android: android::AndroidPad,
    /// The virtual pad of `horizon-ctl pad`.
''',
    'android: android::AndroidPad',
)

replace_once(
    pad,
    '''#[cfg(feature = "controllers")]
mod hardware;

/// A build without the `controllers` feature''',
    '''#[cfg(feature = "controllers")]
mod hardware;

#[cfg(target_os = "android")]
mod android;

/// A build without the `controllers` feature''',
    'mod android;',
)

replace_once(
    pad,
    '''impl Pads {
    /// A new player entity took over''',
    '''impl Pads {
    /// Every physical controller: gilrs on desktop, the Android DeviceEvent
    /// adapter on a phone. The virtual `horizon-ctl pad` remains merged later.
    fn read_all(&mut self) -> PadState {
        let hardware = self.read();
        #[cfg(target_os = "android")]
        let hardware = hardware.merge(&self.android.state);
        hardware
    }
}

impl Pads {
    /// A new player entity took over''',
    'fn read_all(&mut self) -> PadState',
)

replace_once(
    pad,
    '        let hardware = self.pad.read();\n',
    '        let hardware = self.pad.read_all();\n',
    'let hardware = self.pad.read_all();',
)

# Keep this hook anchored only to Game's ApplicationHandler method signature.
# The first statements in device_event change frequently upstream (offscreen,
# focus, save-state checks), but Android controller events must always be
# consumed before whatever mouse-motion handling follows. Anchoring to those
# statements made the overlay needlessly brittle.
replace_once(
    game,
    '''    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
''',
    '''    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        // Android's patched winit backend emits controller buttons/axes as raw
        // device events. Consume those before the mouse-motion path.
        #[cfg(target_os = "android")]
        if self.pad_device_event(&event) {
            return;
        }
''',
    'if self.pad_device_event(&event)',
)

# The same Android input compatibility stage also supplies a portable key-label
# learning fallback because winit's modifier_supplement extension is desktop-only.
subprocess.run(
    [sys.executable, str(port / "tools/patch-android-keybinds.py"), str(root)],
    check=True,
)

print("patched Horizon Android controller adapter")
