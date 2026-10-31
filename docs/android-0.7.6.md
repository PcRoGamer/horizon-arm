# Horizon Android 0.7.6

This update targets touch controls, initial menu navigation and diagnostic recording.

- Optional **Swap hands button**, off by default. It uses the normal swap-hands action and supports the layout editor.
- **Raise shield with: Sneak / Button**. The separate button appears when a blocking item is available. Shield input selects the blocking hand without consuming the main-hand item or interacting with a targeted block.
- Tap-control schemes show a crosshair for bows, crossbows, tridents and throwable items. Their use gestures aim through the center; other items retain touch targeting.
- **Horizontal sensitivity** and **Vertical sensitivity** calibrate camera input independently. Equal physical drag distances now produce equal default turns on both axes, including portrait screens.
- Flint and steel, fire charges, bone meal, buckets and other surface actions use short taps. Food, charged weapons and throwable projectiles retain hold-to-use behavior. Ordinary building blocks keep tap-to-place / hold-to-mine.
- Touch, gyro and controller camera movement count toward the look-around tutorial. Completed tutorial stages remain saved across worlds; an unfinished stage may appear again.
- Hardware D-pad navigation works in the initial title menu before the full game loads. Focus acquisition retries after widgets become available.
- GPU profiling and performance recording share one report session. Both launcher controls use the same button style. Reports include the requested profiling mode and whether encoder timestamps are supported; unsupported hardware cannot provide these stage timings.
- Installation cleanup waits for active resource preparation to finish; the final asset index is saved from verified memory rather than reopened from Android’s evictable cache. This prevents a concurrent startup cleanup from deleting files needed by a reinstall (#30). The original device report does not include a log, so a clean installation on that affected device still needs verification.
- Stable upstream history merged through `7aa0e28fa` (673 incoming commits, including merge history): rendering, chunk work, resource packs, gameplay, network and mod API changes. Android input, presentation bounds, upload budgets and sandbox `.hmod` support are retained.

Native `.hnmod` compilation on Android remains unsupported. Java Fabric, Forge and NeoForge mods are not supported.

## Validation

Local checks passed: 43 touch tests (including layout migration and per-axis calibration), 8 installer tests, 9 diagnostics/timing tests, 26 content tests (one fixture-dependent test skipped), 13 Android tooling tests, ARM64 Clippy and ARMv7 cargo check. Public-tree and whitespace checks passed.

The change is a release candidate until the device checklist below passes. Local source tests do not establish performance or compatibility on every phone.

See [the test checklist](android-0.7.6-testing.md).
