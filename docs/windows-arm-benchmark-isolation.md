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
