# What the port changes

Eight patches on a pinned upstream commit, plus one new crate and two small patches to the pinned
winit. Everything else is upstream as it is, so the world generation, the simulation and the parity
harnesses stay untouched and keep their meaning.

| # | Where | What | Why |
|---|---|---|---|
| 1 | `crates/horizon-client/src/{lib.rs,main.rs,android.rs,game.rs}` | `extern crate self as horizon_client;`, `pub mod game;`, `#[cfg(android)] pub mod android;`; a `OnceLock<AndroidApp>`; `fn event_loop()`; `HORIZON_RESOURCES` and `HORIZON_WORLD`; no argv on Android | Android starts a **library**, not a program: `NativeActivity` loads `libhorizon_android.so` and calls `android_main`. The game loop lived in the bin (`src/game.rs`), unreachable from a library, so it moves into the lib; its `horizon_client::…` paths keep resolving through the `extern crate self` alias, and the desktop binary becomes a thin entry point over the same library. winit builds its event loop from the `AndroidApp` the system hands to `android_main`, so the wrapper stores it for the game. An APK's process has no command line of its own (its argv is the zygote's), so options come from the environment instead of `--resources`, and the argv is not parsed at all — an unknown token there would otherwise end the start; `HORIZON_WORLD` opens a save the way `--world` does. |
| 2 | `crates/horizon-client/Cargo.toml` | winit feature `android-native-activity` | No Java in the APK: the framework's own `android.app.NativeActivity` starts the library. |
| 3 | `crates/horizon-client-input/{Cargo.toml,src/clipboard.rs}` | `arboard` only off Android; the in-game clipboard stays | `arboard` has no Android backend. Copy and paste inside the game work; the system clipboard is `ClipboardManager` and needs JNI, which is the next step. |
| 4 | `Cargo.toml`, `crates/horizon-android/*` (new) | The `horizon-android` crate: `android_main`, the smoke screen, `build.rs` linking `liblog` | The APK needs an entry point that sets `HORIZON_HOME`, finds resources, opens the log and runs the game; on desktop the crate compiles empty. |
| 5 | `crates/horizon-client/src/game.rs` | `Game::touch` | A touchscreen is a pointing device: a tap moves the cursor and clicks, a drag turns the view when the world is captured. Menus work with no Android code of their own. |
| 6 | `crates/horizon-client/src/game/{pad.rs,pad/android.rs (new),game.rs}` | Android key codes and axis ids → `PadState` | winit has no gamepad API and its Android backend drops the keys it has no `KeyCode` for. The winit patch below sends them as `DeviceEvent`s; this maps them onto the controller state the desktop backend already feeds. |
| 7 | `crates/horizon-client/src/game/{touch.rs (new),game.rs,remote.rs}` | The on-screen controls: stick, look area, Jump/Sneak/Hit/Use/Items/Chat/Menu, hotbar slots | A phone needs controls of its own. They feed the same key presses and mouse buttons the keyboard does, so rebinds and screens need no Android code; the stick feeds `Keys::analog`, the same analog move a controller's left stick feeds. The stick also reaches the connected (single-player) path. |
| 8 | `crates/horizon-client/src/game.rs` (`inventory_press`) | Android only: a tap on the empty space around a container closes it | A phone has no Escape in reach while a screen is open. The tap asks `key_pressed(Escape)`, so it closes what Back closes, and only with an empty hand — the surrounding tap still throws a carried stack. |

New crate `crates/horizon-android` (all of it `#![cfg(target_os = "android")]`):

* `src/lib.rs` — `android_main`: points `HORIZON_HOME` at the app's own folder, unpacks the data the
  APK carries, finds prepared resources, opens the log, installs the crash reporter, hands the
  activity over, runs the game; if there are no resources yet, runs the smoke screen instead of dying.
* `src/bundled.rs` — the APK's own `assets/resources/26.3` read through the NDK's `AssetManager` and
  unpacked into the app's files folder on the first run (see below).
* `src/smoke.rs` — winit window + wgpu surface + a cleared frame, with the adapter, the surface format
  and a frame counter in logcat.
* `build.rs` — links `liblog`.

## The winit patches

winit is patched in the cargo registry, not forked (`tools/patch-winit.sh`, idempotent, run by
`tools/build.sh`):

* `android/winit-0.30.13-android-gamepad.patch` — a controller's buttons and sticks arrive as
  `DeviceEvent::Button` and `DeviceEvent::Motion` instead of being dropped or turned into phantom
  touches.
* `android/winit-0.30.13-android-back.patch` — Android's **Back** arrives as `Escape` (physical and
  logical), so it closes the open screen: the inventory, the pause menu, the title's options.

## Input on a phone

* **Touch** — in the world: a floating stick on the left (analog movement, sprint past the deadzone),
  the rest of the screen turns the view, a tap with no travel hits what the crosshair is on; buttons
  for Jump, Sneak, Hit, Use, Items, Chat and Menu; a tap on the hotbar picks the slot. In menus the
  finger is a plain pointer; a tap on the empty space around a container closes it.
* **Keyboard** — a Bluetooth or USB keyboard works as on the desktop (movement, chat text, debug
  keys). In the emulator the AVD must have `hw.keyboard = yes` (the default is `no`, which is why the
  host keyboard looks dead); `tools/emulator.sh` sets it.
* **Controller** — a Bluetooth/USB pad's buttons, sticks, D-pad and triggers go through the same
  controller support the desktop has. Not tested on a real device yet.

## What the port deliberately does not do yet

* **The soft keyboard.** winit's Android backend does not deliver IME text commits, so chat is typed
  on a hardware keyboard only. The on-screen `Chat` button opens the chat screen; the text box needs
  a JNI bridge to `InputMethodManager` (or a small Java `EditText` overlay) to work on a bare phone.
* **Audio.** Built with `--no-default-features`: `cpal` has an NDK backend, but it is untested here and
  would add another failure mode to the first run. `audio` is a feature away.
* **Code mods.** `horizon-client/mods` is off: that is wasmtime + Cranelift, a JIT. Fine on Android,
  impossible on iOS, and it is a quarter of a clean build — not something to debug on a phone first.
* **The launcher.** GPUI is desktop-only. This port has no launcher; resources arrive by `adb push`
  or, later, by a small import screen.
* **The SDK's `aarch64` float path.** `horizon_core::jmath` takes its `aarch64` branch on a phone, which
  is the branch the Mac mini has been checking against vanilla — good news for world determinism. On an
  **x86_64** Android (Waydroid on an Intel host, emulators) it takes the `libm` path instead, so a world
  generated there can differ from a desktop's in the last bit. That is a Waydroid-only caveat, not a
  phone one.
