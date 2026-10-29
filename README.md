# Horizon for Android

**Minecraft: Java Edition 26.3, running natively on Android.**

[Horizon](https://github.com/horizon-rust/horizon) is Minecraft rewritten in Rust — the same world
generation, the same simulation, the same rules, rendered with `wgpu` instead of Java and OpenGL.
This branch is the port that makes it run on a phone: **no emulator layer, no Java, no translation**.
The game's own Rust code, compiled for `aarch64-linux-android`, loaded by Android's `NativeActivity`.

```
   ┌──────────────────────────────────────────────────────────┐
   │  Horizon (upstream, Rust)                                │
   │    worldgen · simulation · physics · mobs · inventory    │
   │    rendering: wgpu → Vulkan / GLES                       │
   └───────────────────────────┬──────────────────────────────┘
                               │  eight commits, one new crate
   ┌───────────────────────────▼──────────────────────────────┐
   │  this branch: the Android port                           │
   │    NativeActivity · touch controls · pads · the APK      │
   └──────────────────────────────────────────────────────────┘
```

> Not affiliated with Mojang or Microsoft. This repository ships no Minecraft files: the game's data
> is built from *your* copy of Minecraft and packed into the APK you build for yourself.

---

## What works

| | |
|---|---|
| **Title screen, world generation, saving** | ✅ playable end to end |
| **Touch controls** | ✅ analog stick, look area, Jump/Sneak/Hit/Use/Items/Chat/Menu, hotbar |
| **Android Back button** | ✅ closes the open screen, like Escape |
| **Hardware keyboard** (Bluetooth/USB) | ✅ movement, chat, F1/F3 debug screens |
| **Controller** (Bluetooth/USB pad) | ✅ buttons, both sticks, D-pad, triggers — untested on real hardware |
| **Single-player** | ✅ on the integrated server thread, as upstream |
| **Multiplayer / LAN** | ✅ the same network code as the desktop |
| **Audio** | ❌ not built in (one Cargo feature away) |
| **Code mods (WASM)** | ❌ off (a JIT and a quarter of the build time) |
| **Soft keyboard for chat** | ❌ winit delivers no IME commits; needs a JNI bridge |
| **Launcher / resource import UI** | ❌ the data is baked into the APK instead |

The interesting part is that almost none of this is Android-specific code in the game. Touch, pads
and keys all feed **the same key presses and mouse buttons a desktop keyboard would**, so key
rebinds, menus and every screen work unchanged.

---

## Install

A phone, one file:

```sh
adb install -r android-port/build/horizon-android-arm64.apk
```

The APK carries the game's data (~320 MB, packed to ~62 MB), so there is nothing else to push. The
first launch unpacks it into the app's own folder behind a loading screen — a few seconds — and every
launch after that goes straight to the title screen.

Saved worlds and `options.txt` live in the app's files folder, so they survive updates.

### Controls

**In the world**

| Where | What |
|---|---|
| bottom-left stick | move (analog: push past the deadzone to sprint) |
| anywhere else | turn the view |
| tap without dragging | hit what the crosshair is on |
| **Jump**, **Sneak** | bottom right |
| **Hit**, **Use** | right side |
| **Items**, **Chat**, **Menu** | top right |
| hotbar | tap a slot to select it |

**In menus** the finger is a plain pointer. A tap on the empty space around a container closes it,
and Android's **Back** does the same as Escape.

**With a keyboard or a pad** nothing changes from the desktop: WASD, chat, the debug screens, and a
controller's sticks and buttons all work as they do on a PC.

---

## Build

Needs Linux x86-64, ~6 GB of disk, and network on the first run.

```sh
android-port/tools/fetch-toolchain.sh     # rustup targets, JDK 21, SDK 34, NDK r27 (~3 GB, once)
android-port/tools/build.sh arm64 fast    # → android-port/build/horizon-android-arm64.apk  (phone)
android-port/tools/build.sh x86_64 fast   # → android-port/build/horizon-android-x86_64.apk (emulators)
android-port/tools/build.sh both fast     # → one APK with both ABIs
```

`fast` is the iteration profile (optimised, quick to link); `release` is the shipping one. The APK is
built with `aapt2`, `zipalign` and `apksigner` directly — no Gradle, and no Java code in it.

The build needs the game's data first (see [Resources](#resources)); `HORIZON_NO_ASSETS=1` produces a
thin APK that expects the data to be pushed separately.

### Try it without a phone

```sh
android-port/tools/emulator.sh --window   # Android 14, x86_64, KVM, a window you can play in
android-port/tools/run.sh                 # installs, starts, prints logcat and the game's log
```

`--software` uses SwiftShader instead of the host GPU, which exercises the GLES fallback.

---

## How the port is built

**The upstream source stays upstream.** The port is eight commits on top of a pinned Horizon commit,
plus one new crate — not a fork of 600k lines with Android calls sprinkled through it.

```
android-port/            everything the port adds outside the game
├── AndroidManifest.xml  NativeActivity, no Java, no Gradle
├── tools/               build, emulator, run, patch, fetch
├── winit/               two patches to the pinned winit
├── docs/port.md         every change, and why it is there
└── LICENSE
crates/horizon-android/  the new crate: android_main, the loading screen, the smoke screen
```

`docs/port.md` in `android-port/` is the full list. In short:

| # | What | Why |
|---|---|---|
| 1 | The game loop moves from the binary into the library | Android starts a **library**, not a program: `NativeActivity` loads `libhorizon_android.so` and calls `android_main`. The loop lived in the bin, unreachable from a library. |
| 2 | The event loop is built from the activity | winit needs the `AndroidApp` the system hands to `android_main`; the game builds its loop deep inside `run()`, so the wrapper stores it. |
| 3 | `arboard` is kept off Android | It has no Android backend. Copy/paste inside the game still works. |
| 4 | The `horizon-android` crate | The entry point: the app's folders, the bundled data, the log, the game — or a smoke screen when there is no data. |
| 5 | The touchscreen is a mouse | A tap moves the cursor and clicks, a drag turns the view. Menus work with no Android code of their own. |
| 6 | Android controllers as device events | winit has no gamepad API, and its Android backend drops the keys it has no `KeyCode` for. |
| 7 | The phone's own controls | The stick, the look area and the buttons, feeding the same keys and mouse buttons the keyboard does. |
| 8 | A tap outside a container closes it | A phone has no Escape in reach while a screen is open. |
| 9 | The data travels in the APK, behind a loading screen | One file to install. |

**Two patches to winit** (`android-port/tools/patch-winit.sh`, applied to the cargo registry copy —
winit is not forked):

* a controller's buttons and sticks arrive as `DeviceEvent`s instead of phantom touches;
* Android's **Back** arrives as `Escape`, so it closes the screen that is open.

### The parts that were interesting

* **Two event loops, one process.** winit's Android backend owns the process's one event loop, and
  the loading screen has to draw *before* the game builds it. So the loading screen uses a raw
  `ANativeWindow` surface with its own wgpu device and no event loop at all — draw, present, repeat.
* **`AAssetDir` lists files, never subdirectories.** The APK's data is unpacked from a flat list of
  paths that `tools/build.sh` writes at build time, into a temporary sibling folder that is renamed
  into place only when every file is in: an interrupted unpack leaves nothing that looks complete.
* **No command line in an APK.** A packaged process has no argv of its own (it is the zygote's), so
  options come from the environment: `HORIZON_RESOURCES`, `HORIZON_WORLD`.
* **The library is stripped.** `rustc` leaves the symbol table in, and it is a fifth of the file:
  59 MB → 47 MB of code, 21 MB → 19 MB of thin APK.

---

## Resources

Horizon ships no Minecraft files, and neither does this repository. `horizon-prepare` builds the data
on a **desktop** from your own Minecraft: Java Edition 26.3 files (the pinned jars are checked
against upstream's `research-lock.json`), and `tools/build.sh` packs the result into the APK.

```sh
cargo run --release -p horizon-prepare -- --prepare --server-jar <server.jar> --root .
```

To keep the data out of the APK instead — for a device that already has it, or to iterate on code
without repacking 320 MB:

```sh
HORIZON_NO_ASSETS=1 android-port/tools/build.sh arm64 fast
adb push .local/horizon-client/26.3 \
  /sdcard/Android/data/dev.horizon.android/files/.local/horizon-client/26.3
```

The port looks in, in order: `HORIZON_RESOURCES`, then `<app files>/.local/horizon-client/26.3`,
then the external folder above, then a couple of older names. A folder that holds
`horizon-resources.json` is a prepared one, and anything already on the device wins over the APK's
copy.

---

## Status

Playable, and the interesting parts are done: it renders, generates, saves, takes touch and pads, and
installs as one file. What is left is the long tail of a phone being a different computer:

* **soft keyboard** — chat needs `InputMethodManager` over JNI (or a small Java overlay);
* **audio** — `cpal` has an NDK backend; untested here, and left out of the first build;
* **battery and thermals** — the frame limiter is still the desktop one;
* **real-device testing** — everything above was proven on an Android 14 x86_64 emulator. A phone
  with an Adreno/Mali GPU and a real Bluetooth pad is the next thing to try.

## Credits

* [Horizon](https://github.com/horizon-rust/horizon) — the game itself, MIT, by Ludovic Marie and
  the Horizon contributors. This port would be nothing without it.
* [winit](https://github.com/rust-windowing/winit) and [wgpu](https://wgpu.rs) — the window and the
  renderer, both of which do the hard Android work.
* Minecraft is Mojang's. This project contains none of their files.

Licensed MIT, like upstream — see [LICENSE](LICENSE). Upstream's own README is
[README.upstream.md](README.upstream.md).
