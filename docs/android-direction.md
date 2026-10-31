# Android client direction

This is an Android fork of Horizon, not a Java mod loader. The Rust world,
simulation, renderer, protocol and most menus come from Horizon. Our work adds
the Android launcher, packaging, resource installation, touch input, system IME,
audio integration and a small set of shared-client fixes and optimizations.
The Android branch now preserves upstream Git history from the best matching
base recorded in [android-upstream.json](android-upstream.json). Use Git diffs
against that commit to review our changes separately from the original engine.

## Compatibility today

| Content | Current Android behavior |
|---|---|
| Native `.hmod` | Wasmtime components work on Android; the coordinates HUD has been exercised in both single-player paths |
| Native mod API | Version 0.1 exposes player/clock reads, HUD, chat, commands, storage and lifecycle hooks; it is not a full gameplay-mod API |
| Resource packs | Supported within the renderer's implemented formats |
| Java mod assets/data | Some content can be carried through the porting pipeline; unsupported behavior must be reported |
| Fabric/Forge/NeoForge code and Mixins | Do not run as ordinary JARs; need native ports or a separate Java execution architecture |
| Vanilla-compatible Java servers | The engine has a Minecraft 26.3 / protocol 777 client; individual server/proxy/anti-cheat compatibility still needs testing |
| Authenticated servers | The engine implements the encrypted login/session join, but the Android launcher has no Microsoft sign-in flow yet |
| Modded servers | Arbitrary custom handshakes, payloads and registry synchronization are not supplied by importing mod assets |

See [mods](mods.md), [Java mod compatibility](research/java-mod-compat.md),
[account sign-in](microsoft-account.md) and [hosting](hosting.md).

## Recommended scope

Keep a native Android fork with a clear upstream relationship. The measured
phone performance makes the Rust path worth preserving. A separate brand does
not solve mod compatibility: that needs APIs, native implementations and tests.
The repository's [MIT license](../LICENSE) includes the original copyright and
notice requirement. Retain that attribution and follow the repository's rule
against committing Minecraft JARs, assets or generated tables.

Prioritize the following work:

1. Recover the source baseline and track Android patches so upstream fixes can
   be reviewed and integrated reliably.
2. Make vanilla-server compatibility and Android account sign-in a dedicated
   track: approved application ID, legitimate Microsoft device-code flow,
   protected token storage and session handoff. Account registration/approval
   requires the publisher; it cannot be replaced with a fabricated client ID.
3. Extend the versioned mod SDK around specific native ports: item/recipe and
   inventory reads, UI/input hooks, then explicit gameplay/network capabilities.
   Keep existing `.hmod` compatibility and capability checks.
4. Publish an honest compatibility matrix for tested mods and servers. Separate
   running native code, carried content and unsupported Java behavior.
5. Keep optimization work measurable and independent of graphics limits:
   frame-time percentiles, allocation counts and streaming stalls at the same
   resolution, settings, world and camera path.

If unchanged Java JAR compatibility is the primary requirement, a Java/JVM
launcher is a different product direction from this Rust-only codebase.
Reimplementing Mixins and the entire Java mod ecosystem inside Horizon is not a
small compatibility layer.

## Optimization update 0.4.1

- Pack transparent-quad distance and stable tie order into integer keys. Use
  in-place sorting for small lists and a retained stable radix-sort buffer for
  larger lists, with ordered/reversed fast paths. Match JOML's arithmetic order
  for nearly equal distances.
- Decode occlusion readback words directly from potentially unaligned mapped
  memory, without allocating and copying a temporary result vector.
- Iterate retained mod HUD lines directly instead of collecting a vector for
  each panel on each frame.

These changes do not lower resolution, distance, FPS limits or visual quality.
Synthetic CPU measurements are not a prediction of FPS on a phone. The
translucency parity gate includes large meshes around the sorting threshold;
unit tests cover ties, non-finite values, ordered runs and unaligned readbacks.
