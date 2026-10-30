# Horizon Android 0.7.5

The published RC is followed by source changes for the next 0.7.5 candidate.

- Native-mod upstream synchronization and Android compatibility fixes.
- Optional virtual mouse: enable its button in Touch Controls, then use Mouse in a menu to hover without clicking. Existing inventory attack/use controls provide clicks.
- Fifth touch scheme: touch gestures aim at the center crosshair without separate attack/use buttons. Short taps with use-items attack; holds use them.
- Gyroscope aiming: opt-in, adjustable sensitivity, display-rotation axis mapping, and sensor shutdown on focus loss (within a bounded 20 ms worker poll). A device without a gyro remains playable. Gyro input applies only to active touch gameplay.
- Preferred display refresh rate: a native `ANativeWindow_setFrameRate` request on Android 11+, with System default restoring the OS choice. It is a preference, not a guaranteed display mode and not the game's FPS cap.
- Optional ADPF performance hints on supported Android 12+ devices. A session tracks the rendering thread's actual CPU-side frame work including submission waits. It uses the explicitly requested display period, or 60 Hz when System default is chosen. Hints close on focus loss. There is no guaranteed performance gain or change to graphics quality.
- Detailed GPU profiling: recording switch plus rolling p50/p95/p99 for preparation/uploads, world, and post/hand/HUD stages, correlated with CPU/streaming and native API diagnostics. Timestamp availability depends on the driver. These are not vendor hardware counters, GPU clock readings, or proof of a slowdown's cause. Profiling has overhead and is off by default.
- Exit reports: Android 11+ historical process exits with OS reasons and available Java/native/ANR traces; a previous-log tail and Rust panic export. Traces are bounded and retained locally. Native crash traces can be binary tombstones. Older firmware or the OS may provide no trace.

Settings are in Options > Controls > Touch Controls. Existing performance recording in launcher Settings remains available. All new sensor/performance controls default to off and display frequency defaults to System default.

Reports are accessible through the Android Files app under Horizon Android > Reports. Native exit history appears after the next launch. Review logs before sharing: they can include file paths, server addresses and exception text. No reports are uploaded automatically. Normal Android low-memory process termination is recorded as such; it is not automatically labeled a game crash.

Device support is detected at runtime. Old Android releases retain normal startup without linking newer performance/frequency symbols at load time.
