# Native Horizon HUD-only renderer

This is **not** the previous browser mockup. This small Android Activity renders the **actual production Java Canvas code** from `HorizonActivity.TouchOverlay`. It has no GameActivity, JNI, Minecraft world, game loop, or renderer dependencies.

The build executes `tools/extract_horizon_overlay.py`: it copies the production drawing class from `packaging/android/java/dev/horizon/android/HorizonActivity.java`, replacing only the class name and constructor's Context argument. All icon masks, buttons, shaders, Canvas calls, dimensions, opacity rules, and joystick drawing methods are unchanged. It fails rather than silently diverge when the production implementation moves.

## Fast iterative cycle

Once, from the repository root, with Android SDK 35, JDK 17+, Gradle 8.14.3, and a booted Android emulator:

```powershell
gradle -p packaging/hud-preview assembleDebug
adb install -r packaging/hud-preview/build/outputs/apk/debug/hud-preview-debug.apk
```

Then, without launching Minecraft:

```powershell
adb shell wm size 1280x800
adb shell am start -n dev.horizon.hudpreview/.HudPreviewActivity --ei scheme 1 --ei scale 100 --ei opacity 35
# Use tools/Capture-HorizonHud.ps1 for safe binary capture in PowerShell
.\tools\Capture-HorizonHud.ps1 -Scheme 1 -Scale 100 -Opacity 35 -Output hud.png
```

Re-running the `am start` command updates the activity immediately. Schemes 0/1 are joystick, 2/3 D-pad; 1/3 use crosshair controls. `--ei buttons 1024` previews the sprint-pressed state. CI also builds the Android harness and captures real screenshots with an emulator on every push to the tools branch.

## What is and is not rendered

**Native Android, source-identical:** chat, pause, sprint, jump, sneak, hit, use, D-pad, joystick, bevels, opacity and pressed states.

**Not rendered yet:** native **Rust GPU hotbar**, inventory end-cap, player items/status bars, and 3D world. These belong to the Rust renderer, so they need a separate native HUD-only render path. We deliberately do not use fake rectangles or a simulated world as substitutes.

The result is a true source-derived Android control-layer render. It is not proof of complete Bedrock HUD parity until the native Rust HUD layer is captured and composited as well.


## Reusing Horizon's existing Minecraft assets

The touch buttons now use **runtime-loaded vanilla Minecraft textures**. No new network downloader or Minecraft-owned PNG is committed to Horizon or bundled in its APK.

- **Button face:** `assets/minecraft/textures/block/stone.png`
- **Normal frame:** `assets/minecraft/textures/gui/sprites/widget/button.png`
- **Highlighted/pressed frame:** `assets/minecraft/textures/gui/sprites/widget/button_highlighted.png`

On Android, the production `TouchOverlay` reads these from `Context.getFilesDir()/.local/horizon-client/<version>/`, which is the same versioned directory used by `bootstrap::resource_root`. First-run preparation may not have finished when the Activity starts, so the UI retains the old functional fallback surfaces and polls for the textures at most once every three seconds. The button press hitboxes do not change.

The Windows/Rust HUD-only preview reads `block/stone` and `gui/sprites/widget/button` through the **existing HudAtlas**. The border is nine-sliced, preserving vanilla menu corner pixels instead of stretching a 200x20 sprite into a square.

The Android Robolectric test writes **synthetic texture fixtures** into the normal cache layout and verifies that the production code loads and renders them. Fixture screenshots prove the code path, **not Bedrock pixel parity**. A final visual comparison with genuine Mojang texture pixels requires a prepared game directory and a running Android build.
