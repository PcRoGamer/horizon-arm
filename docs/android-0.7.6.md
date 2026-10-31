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
- Stable upstream history merged through `c3905131b` (717 incoming commits, including merge history): rendering, chunk work, resource packs, gameplay, network and mod API changes. Android input, presentation bounds, upload budgets and sandbox `.hmod` support are retained.

Native `.hnmod` compilation on Android remains unsupported. Java Fabric, Forge and NeoForge mods are not supported.

The #34 Mali-G52 startup report identified a 1024-layer hardware limit with 1291
sprites. Renderer construction now selects a supported per-array size and
regroups the already prepared mip bytes across existing arrays. Shader packs use
the same actual split. Texture resolution, mipmaps and colors are retained;
verification on the originally affected phone is still required.


## Release fixes

- Periodic CPAL device discovery runs on a worker, at most once per second even in the launcher. Normal frame ticks only poll its result; reopening an actually changed audio output still occurs when needed.
- Diagnostic heartbeat/logging skip a busy state lock; redaction, memory collection and writes run on the worker. Logger flush no longer waits for disk during rendering/startup. Reports exposes ZIP exports and performance JSONL, not private session logs or raw OS traces.
- Installer finalization calls cleanup while retaining its existing transaction lock, avoiding recursive mutex acquisition (PR #39). Metadata requests retry transient failures; completed files remain reusable.
- Integrated-server saves write the owner's state again. A dedicated-server compatibility flag must not suppress singleplayer inventory, position and experience persistence.
- Android prefers opaque surface composition; where unavailable, the final output makes alpha opaque without changing RGB or internal shader/HDR data (#37). Original-phone verification is still required.
- OpenGL ES selection also applies to the launcher. An interrupted first Vulkan frame triggers OpenGL ES recovery on the next launch, with an explanatory status (#38). This does not identify or repair every vendor-driver crash.
- GLES sprite selection preserves the newly added texture-filtering functions; assembled shader validation and DirectX translation pass. The storage probe follows the relocated file-lock module.
- Fireworks no longer enable the automatic projectile crosshair. Swap hands moves away from Sprint; only the original factory placement is migrated.

## Crash diagnostics

- Always-on bounded local diagnostic log, independent of performance recording.
- Current/previous process markers with build/ABI, last startup/game phase,
  a five-second memory heartbeat and handled game errors. An unclean marker is
  evidence of interruption, not proof of a crash: Android may kill a process.
- Android 11+ exit collection runs on a worker. Reports distinguish native/Java
  crashes, ANR and low-memory kills, retain OS memory samples and explicitly report
  unavailable/failed collection. Trace availability depends on Android.
- Settings > Diagnostic report exports a local ZIP and opens Reports. It includes
  bounded log tails, exit history, available panic reports and readable summaries
  of the crashed native thread (module/PC/Build ID). Raw tombstone memory/register
  dumps, accounts and worlds are not included. Redaction is best effort: review
  before sharing. Nothing is uploaded automatically.
- Release packaging keeps matching private native symbols under
  `.local/android/native-symbols/`, outside APKs and release attachments. Final
  Android libraries carry an ELF Build ID for matching native crash frames.

See [diagnostic collection and limitations](android-diagnostics.md).

## Validation

Local checks passed: 2 execution-role/player save tests, 44 touch tests (including layout migration and per-axis calibration), 8 installer tests, 5 crash-diagnostic tests, 9 performance/timing tests, 27 content tests (one fixture-dependent test skipped), 10 audio tests, 19 GPU tests, 26 renderer/shader tests and a standalone storage-probe build, 13 Android tooling tests, ARM64 Clippy and ARMv7 cargo check. Public-tree and whitespace checks passed.

The change is a release candidate until the device checklist below passes. Local source tests do not establish performance or compatibility on every phone.

See [the test checklist](android-0.7.6-testing.md).

## Post-RC1 allocator candidate

Phone reports from RC1 show frame-thread `arena_buffer_create` spikes of 799.5 ms,
571.0 ms and 364.4 ms. These are nested inside geometry upload, not evidence of
CPU time spent in diagnostic serialization. Android arena pages now stop normal
growth at 8 MiB rather than repeatedly growing/copying buffers toward 256 MiB.
Large single meshes retain the original hardware limit. Desktop allocation policy
is unchanged. Twenty GPU tests pass, including range/data preservation across
bounded pages and a larger single mesh. Phone FPS and allocator-stall improvement
remain unverified; this is not a benchmark claim.
