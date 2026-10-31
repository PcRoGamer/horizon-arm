# Android development port

Horizon runs its Rust simulation and wgpu Vulkan renderer in Android GameActivity.
This build implements **Minecraft 26.3**. It is not a launcher for arbitrary Java
versions.

## Install and play

Install the ARM64 APK on an Android phone (Android 9+, Vulkan), or the x86_64 APK
in an x86 Android emulator. Open the app and tap **Download 26.3**. The app fetches
Minecraft's version metadata, client resources and supplemental assets directly
from Mojang over HTTPS, verifies SHA-1 and file sizes, then installs atomically.
The first download is approximately 501 MiB, including official sound assets.
Tap **Play** after installation. Subsequent launches use installed resources
offline. No computer, local server, ADB forward, or phone Java runtime is needed.
Use your own licensed Minecraft copy.

The APK contains the native engine and version-specific generated engine tables.
Game textures, models, fonts, languages and menu backgrounds are downloaded from
Mojang. The engine tables are prepared by the developer during APK creation,
not by the player. Updating the implemented Minecraft version requires an engine
update, not just choosing another client JAR.

Saves and download caches live in the app's internal data directory. Download
failures retain verified cache entries for retry; partial files are never treated
as installed resources. Uninstalling removes app data.

## Touch controls

Version 0.5.0 implements the three interaction schemes named in the official
[Bedrock changelog](https://www.minecraft.net/en-us/article/minecraft-1-21-60-bedrock-changelog):

- **Joystick & aim crosshair**: analog movement, camera drag, attack/mine and
  use/place buttons, jump, sneak and sprint.
- **Joystick & tap to interact**: tap a world point to use/place, tap an entity
  to attack, hold to mine. Held consumables, shields and ranged items use the
  held-use action instead. Dragging turns the camera and cancels a world tap.
- **D-Pad & tap to interact**: directional movement with the same world gestures.
  Enable **Split controls** for crosshair aiming and action buttons.

The top-center row contains chat, pause and a gear for **Touch settings**.
Open the gear while playing to choose a scheme, select and drag controls, and
adjust the selected control's size and opacity with sliders. Sensitivity has
its own slider. Left-handed mode mirrors positions; auto sprint is optional.
In D-pad mode the center button toggles sneak; Swap jump/sneak exchanges it
with the jump button. Zero opacity hides a control while the editor still
shows a faint preview so it can be recovered.
Each scheme keeps an independent layout. Done saves `horizon-touch.json` in the
current profile; Cancel/Android Back restores the previous settings, and Reset
restores the current scheme's positions, sizes and opacity. Single-player
pauses during editing; a remote server continues running. No game actions are
sent by editor touches. Switching off controller support does not disable touch.

The nine hotbar slots select items; the movable ellipsis opens inventory.
The use button also appears for entity/item interactions in tap modes. These
controls map to existing Horizon actions, including flight/swimming movement,
controller rebinding and toggle sneak. They do not replace vanilla gameplay.
Version 0.5.1 replaces the drawn icons with original normal/pressed PNGs from
[Mojang bedrock-samples](https://github.com/Mojang/bedrock-samples/tree/46ba6ea985fb5a92d79a9419198f10dda14c199d/resource_pack/textures).
Flight, swimming, dismount and diagonal D-pad states use their original sprites.
Texture bytes remain private in `.local/`, with original licensing/provenance.
This is an adaptation of Bedrock's schemes, not verified pixel-for-pixel parity:
emotes, every mount-specific icon, inventory stack-splitting gestures and all
Bedrock touch options are not implemented. Projectile launch direction still
follows the engine's player view; use crosshair mode for precise ranged aiming.

Menus support tapping, list swipes, slider dragging and inventory dragging.
The **Back** button closes inventory/screens; Android Back closes the native
keyboard first. The compact **...** drawer retains F1-F12, F5 perspective,
chat, the native keyboard, swap/drop and other Java keys. Hold different keys
with separate fingers, or latch **HoldF3** before another F-key. F3+F6 remains
an upstream engine stub; the key is delivered but no Debug Options screen exists.

## Android platform features

The existing controller **Vibration** switch now controls the phone's vibrator.
Hurt, block destruction, explosions and totem events use Horizon's existing
pulse definitions through Android's [VibrationEffect](https://developer.android.com/reference/android/os/VibrationEffect).
No vibrator is a supported case. Vibration stops on suspend/focus loss and when
the option is disabled. Actual feel and amplitude need phone testing; emulator
startup cannot establish physical vibration quality. External-controller rumble
and DualSense adaptive triggers are not supplied by the phone-vibrator backend.

Horizon's copy/paste now uses the Android [system clipboard](https://developer.android.com/develop/ui/views/touch-and-input/copy-paste)
for explicit text-edit shortcuts and copy actions. Text-only clipboard access
avoids resolving file/content URIs. Gboard's normal paste UI also remains usable.
AAudio, native Unicode IME, lifecycle surface recreation, native `.hmod` mods,
resource packs and the engine's graphics settings are compiled in. See the
[Android feature audit](android-features.md) for remaining platform gaps.

## Mobile launcher

The native launcher now has Play, Mods, Packs, Modpacks, Installed and Settings
pages. The mobile UI shares the desktop launcher's actual `launcher/src/browser`
backend: Modrinth search, version selection, dependency resolution, checksum
verification, Java content compatibility checks, install and remove.

- **Mods** searches Java content mods. Their data/assets may port; their Java,
  Fabric/NeoForge mixins and behavior do not run. Installation reports unsupported
  projects instead of treating a downloaded JAR as a running mod.
- To import a native Horizon mod, paste its direct **HTTPS `.hmod` URL** into the
  Mods search field and press Search/Done. HTTPS is enforced through redirects,
  the download is capped at 32 MiB, and its manifest/package is validated. New
  native mods remain disabled until enabled in Installed. Only use sources you
  trust; these direct downloads are not an authenticated Horizon mod index.
- **Packs** downloads resource packs. Installed toggles them in the profile's
  options; removing a pack also removes its enabled entry.
- **Modpacks** downloads and runs the existing Rust import/inventory/port/report
  pipeline. Choose an imported pack with Play's Modpack profile selector. Only
  non-conflicting carried content is loaded. A pack with no loadable content is
  rejected. This does not make arbitrary Java modpacks compatible.
- **Installed** manages browser additions and native `.hmod` enablement. The
  game's Options > Mods shows each mod's capabilities and runtime errors.
- **Settings** selects a rendering mode, profile, offline player name and server.
  Rendering modes and profiles use explicit selection lists. Modpacks have a
  separate selection list; installed additions have switches showing their state.
  Profiles can be created without the old five-profile limit. Profile zero keeps
  existing Android saves. Name and server fields use the system keyboard.
- Graphics options (resolution, chunk distance, FPS, quality and worker budgets)
  belong to the game's settings. The launcher does not rewrite `options.txt` or
  inject a render-distance argument. Old launcher graphics fields are ignored on
  upgrade; existing game settings are preserved until the player changes them.

This is an Android UI and a port of the content/launch functionality, not full
desktop-launcher parity. Desktop binary/self-update installation, OS keychain
account integration, desktop agent tools and native Android file-picker import
are not implemented. APK upgrades are manual. Java touchscreen mods such as
[MidnightControls](https://github.com/TeamMidnightDust/MidnightControls) require
a source port to Horizon's Rust input layer; recompiling their JAR is insufficient.
The auxiliary-key design was checked against
[Pojav's controls](https://github.com/PojavLauncherTeam/PojavLauncher/blob/v3_openjdk/app_pojavlauncher/src/main/java/net/kdt/pojavlaunch/customcontrols/CustomControls.java).

## Android terrain compatibility

By default Android uses direct per-section terrain draws and disables GPU occlusion
culling, indirect first-instance/multi-draw and mappable primary buffers. This
avoids desktop-oriented paths implicated when entities render but terrain does
not. It is a compatibility mitigation for phone Vulkan drivers, not a confirmed
diagnosis for every device. The terrain path is tested on the emulator; the
reported POCO M8 Pro symptom still needs verification on that physical device.
The desktop renderer keeps its existing feature selection. Both modes use the GPU.
The rendering selection names both actual paths: **Vulkan / GPU batching**
(adapter-supported indirect draws and GPU culling) and **Vulkan / compatibility**
(direct terrain draws). These use the same system Vulkan driver and API. There
are no bundled alternative OpenGL, ANGLE or custom Turnip drivers in this build.

## Performance and sound

APKs use the optimized Rust `fast` profile. Graphics limits and presets are
controlled by the game itself, including the existing resolution slider. The
launcher no longer applies the old 50% / 60 FPS / four-chunk mobile preset.
Changes made by that earlier preset stay as ordinary game settings; resetting
those automatically would overwrite player choices.

The F3 overlay has its own vanilla `debugGuiScale` option in the profile's
`options.txt`: `0` uses the automatic debug scale, `-1` follows the GUI scale,
and positive values request an explicit scale (bounded by the GUI scale).
Horizon currently has no Debug Options screen for changing this visually.

Audio is compiled in and the installer downloads the official sound assets with
checksum verification. An upgrade from an older silent APK prompts for resource
installation again; existing cached objects are reused and saves stay in place.
Android GameActivity supplies the system IME, including Unicode and composition;
no custom Android keyboard or additional keyboard/audio permission is required.

## Build (developers only)

Install Rust, Python 3.11+, Android SDK (platform 35+, build tools), NDK, and a JDK
for APK signing, and Gradle 8.14. Android Studio's bundled JDK works.
GameActivity 4.4.0 and AppCompat are resolved from Google Maven by Gradle. Prepare the pinned engine
tables once on a development computer, then build:

```sh
rustup target add aarch64-linux-android x86_64-linux-android
python tools/android_prepare.py
python tools/android_bedrock_controls.py
python tools/android_build.py --abi arm64-v8a
python tools/android_build.py --abi x86_64
```

Windows discovers `%LOCALAPPDATA%/Android/Sdk` and Android Studio's JDK. Elsewhere
pass `--sdk PATH --java-home PATH` or set `ANDROID_HOME` / `JAVA_HOME`. APKs go to
`.local/android/apk/<abi>/horizon-android.apk`, signed with the development key in
`.local/android/debug.keystore`. The default `--profile fast` is optimized;
`--profile dev` is for debugging. `--gradle PATH` overrides cached Gradle discovery.
The build packages generated tables into
`assets/engine-data.zip`, excluding game assets, Java classes and game JARs.
Keep all generated tables and APKs private in `.local/`; never commit them.
No `bootstrap.json` or local download URL is required or embedded.

The APK is debuggable. Cleartext traffic is disabled, the resource downloader
accepts only official Mojang HTTPS hosts and does not follow redirects. Every
client and asset file is checksum-verified before use.

## Check

```sh
python tools/android_build.py --abi x86_64 --check
python tools/android_build.py --abi x86_64 --clippy
cargo test -p horizon-client --lib --no-default-features touch
cargo test -p horizon-client --lib --no-default-features ime_edit::tests
cargo test -p horizon-client --lib --no-default-features official_install::tests
python tools/android_ctl.py --serial SERIAL state
```

The control helper reads the debug app's private descriptor through `run-as`,
forwards one loopback port for one command, then removes it. It never prints the
authentication token. This forward is only for development control, not downloads.
For in-frame smoke tests, place an argument array such as
`["--in-frame", "--world", "Android In Frame", "--creative"]` in internal
`files/android-launch.json`, restart and tap Play. Remove it afterwards to restore
the default server-thread mode.

Use fresh AVDs and fresh game/launcher/XDG folders for agent tests, with
`HORIZON_LAUNCHER_NO_SELF_UPDATE=1` and `HORIZON_NO_AUDIO=1`. Never test on existing
player data. An emulator's host GPU is not evidence of physical phone performance.
