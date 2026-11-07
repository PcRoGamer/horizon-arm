# Horizon ARM64 controlled benchmarking on an active Windows desktop

## Processor partition

The Snapdragon X Elite X1E-78-100 laptop has 12 cores arranged in three 12 MiB
shared-L2 groups: 0-3, 4-7, and 8-11 (verified via Windows topology API).
Initial controlled profile: Horizon on 0-7; selected background apps on 8-11.
Compare other partitions using actual frame data before adopting one.

## Tool choice

Process Lasso native ARM64 by Bitsum is the preferred mature desktop policy
manager. It supports affinity, CPU Sets, process and I/O priority, and GPU
priority. Official download: https://bitsum.com/download-process-lasso/
The native Governor requires elevation on this laptop. The stable installer
was obtained and signature-checked; it was not activated or installed.

tools/bench_desktop_isolation.py is a non-elevated, self-restoring benchmark
adapter. It moves ONLY allowlisted desktop applications to background cores,
launches Horizon through tools/bench_resource_envelope.py with a Job Object
restricted to its own cores, and restores all original affinities on exit.
An independent watchdog restores them after controller termination.
System services, UniSync, and Python used by UniSync must not be restricted.

## Running

From the repository root, inspect candidates before applying changes:

    python tools/bench_desktop_isolation.py --dry-run

Run an already-built optimized Windows ARM64 Horizon executable:

    python tools/bench_desktop_isolation.py --horizon-cores 8 --timeout 1800 --report artifacts/benchmarks/isolated-arm64.json -- C:\path\to\horizon-client.exe --resources C:\path\to\resources

Optional: --background firefox.exe,chrome.exe,code.exe for a custom allowlist.
The default allowlist is these three processes. Use --cpu-cap and --memory-mb
only when a particular comparison requires those additional limits.

## Validity gates

A hard CPU cap is a maximum, not a reservation. GPU work, shared memory
bandwidth, OS and driver threads, DPCs, paging, and thermals remain shared.
Store the isolation report, its companion *.envelope.json, and Horizon's
per-frame timings. CPU usage alone is not an FPS benchmark.

Use identical scene, build profile, render backend, driver, frame cap and
resolution for all candidates. Alternate builds across repeats; measure
P50/P95/P99 frame times, 1% and 0.1% lows, startup, memory and sustained
thermals. Mark contaminated runs, including unexpected GPU contention,
focus changes, uncontrolled CPU spikes and failed affinity operations.
Never present sanitized results as unrestricted user performance. Maintain
a separate normal-use benchmark to quantify expected real-world behavior.

## Tests

    python tools/test_bench_desktop_isolation.py -v

The tests only modify a uniquely named temporary executable derived from
the Windows ping utility. They cover normal restoration and forced-crash
watchdog restoration. A separate live smoke test confirmed that 28 browser
processes were temporarily isolated and all restored with zero errors.

The existing tools/perf_bench.py provides repeatable gameplay scenarios and
statistics; reuse it for actual FPS testing rather than introducing a second
gameplay benchmark suite.

## Native ARM64 toolchain for the Windows benchmark

The Snapdragon laptop's global Rust default was an x86-64 GNU host compiler;
the native compiler avoids that emulation cost. Install native Rust without
changing the user's default toolchain:

    rustup toolchain install stable-aarch64-pc-windows-msvc --profile minimal

The project also needs the Visual Studio ARM64 compiler environment and
Clang for the ring crate. Verified portable native LLVM 23.1.5 is under:

    artifacts/toolchains/llvm-23.1.5-win-arm64/clang+llvm-23.1.5-aarch64-pc-windows-msvc/bin

Official release archive: llvm/llvm-project, llvmorg-23.1.5,
clang+llvm-23.1.5-aarch64-pc-windows-msvc.tar.zst
SHA-256: cb523582d9fe00862c5584ce5f7715f963037c11bba95181fa98425f48e06863

The native compiler successfully compiled a C source including assert.h with
Visual Studio vcvarsarm64.bat active. That smoke test alone does not certify
a complete Horizon executable.

The tools/Run-WindowsArmFast.ps1 script selects native ARM64 Rust and the
portable verified LLVM when it needs to build. It does not change global
Rust preferences. The project's fast profile is convenient for iteration;
the fully optimized release profile must also be benchmarked before
asserting maximum runtime performance.

## Controlled gameplay runner

    python tools/bench_snapdragon_arm.py --check
    python tools/bench_snapdragon_arm.py --preset smoke
    python tools/bench_snapdragon_arm.py --preset standard

The --check preflight requires an actual built optimized ARM64 executable.
The smoke preset runs one deterministic forest flight at 1280x720, render
distance 8. The standard preset runs repeated village/forest/ocean/mountain
flights at two render resolutions and render distances. Both enforce the
eight-core Horizon / four-core desktop partition.

The runner uses offscreen DX12 by default to avoid stealing window focus.
This isolates many window-system effects but measures a distinct workload
from fullscreen on-screen presentation. Keep an additional explicitly
labelled visible gameplay benchmark for display-frame pacing. Never label a
debug build as release-speed.

Horizon itself produces frame-time reports in benchmarks/perf, including
percentiles and low FPS values. The launcher saves an immutable binary hash
and run settings under artifacts/benchmarks/<label>/manifest.json. The
matching isolation.json, isolation.envelope.json, isolation.gpu.jsonl, and
run.log retain the environmental evidence.

tools/bench_gpu_activity.py uses native Windows PDH counters to record 3D
GPU activity by process every second. It reports Horizon, desktop compositor,
and other applications separately. This is a contamination monitor, NOT
dedicated GPU capacity or a precise GPU reservation. The monitor keeps raw
JSONL samples for correlation with Horizon's flight-time windows.

The benchmark's Windows control socket must be a localhost TCP JSON endpoint
even though modern Windows Python has AF_UNIX. A regression test enforces
this distinction against crates/horizon-client/src/control.rs.

Validation commands:

    python tools/test_bench_desktop_isolation.py -v
    python tools/test_bench_gpu_activity.py -v
    python tools/test_bench_snapdragon_arm.py -v
    python tools/test_perf_bench_control_path.py -v
