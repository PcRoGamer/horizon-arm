#!/usr/bin/env python3
"""Repeatable native Windows ARM64 Horizon gameplay benchmark on an active laptop.

Requires an already-built optimized ARM64 horizon-client.exe and prepared
Horizon resources. Runs the repository's deterministic performance harness
under the crash-safe 8+4 desktop isolation controller, with DX12 and offscreen
rendering (no desktop focus theft). Captures GPU interference independently.

Use --check first for a no-side-effect preflight; --preset smoke makes one
short flight, --preset standard performs a multi-scene/multi-resolution run.
This benchmark is NOT a general Windows CPU microbenchmark and NEVER reports
synthetic CPU load as Horizon FPS.
"""
from __future__ import annotations

import argparse
import atexit
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import threading
import time
import zipfile

from bench_windows_exclusive import BenchmarkLease, running_clients

ROOT = Path(__file__).resolve().parents[1]
PERF = ROOT / "tools" / "perf_bench.py"
ISOLATION = ROOT / "tools" / "bench_desktop_isolation.py"
PRESETS = {
    "smoke": {"sizes": "1280x720", "distances": "8",
              "scenes": "forest", "seconds": 5.0, "repeats": 1, "timeout": 900},
    "standard": {"sizes": "1280x720,1920x1080", "distances": "8,16",
                 "scenes": "village,forest,ocean,mountain", "seconds": 12.0,
                 "repeats": 3, "timeout": 7200},
}


def pe_machine(binary: Path):
    with binary.open("rb") as f:
        if f.read(2) != b"MZ":
            raise ValueError("Executable is not a Windows PE image")
        f.seek(0x3c)
        offset = int.from_bytes(f.read(4), "little")
        f.seek(offset)
        if f.read(4) != b"PE\0\0":
            raise ValueError("Executable has an invalid PE signature")
        return int.from_bytes(f.read(2), "little")


def sha256(path: Path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(8 * 1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def resources_candidate():
    return [
        ROOT / ".local" / "horizon-client" / "26.3",
        ROOT.parent / "horizon-android-polish" / ".local" / "horizon-client" / "26.3",
    ]



def prepare_benchmark_client_jar(resources: Path, dry_run: bool):
    """Reuse a pinned, already-downloaded client jar, never fetch or publish it.

    The performance harness accepts --client-jar and reads version.json
    directly. Locate an existing owned JAR, validate its pinned version,
    and do not copy or hardlink assets across worktrees.
    """
    version = json.loads((ROOT / "research-lock.json").read_text(encoding="utf-8"))["minecraft"]["version"]
    dest = ROOT / ".local" / "minecraft" / version / "jars" / "client.jar"
    candidates = [
        dest,
        resources.parents[1] / "downloads" / version / "client.jar",
        resources.parents[1] / "minecraft" / version / "jars" / "client.jar",
    ]
    for sibling in ROOT.parent.glob("horizon-*"):
        candidates.extend([
            sibling / ".local" / "downloads" / version / "client.jar",
            sibling / ".local" / "minecraft" / version / "jars" / "client.jar",
        ])
    for source in candidates:
        if not source.is_file():
            continue
        try:
            with zipfile.ZipFile(source) as zipped:
                info = json.loads(zipped.read("version.json"))
            if info.get("id") != version or not isinstance(info.get("world_version"), int):
                continue
        except (OSError, ValueError, KeyError, zipfile.BadZipFile):
            continue
        # --client-jar passes the owned JAR straight to the performance
        # harness. Never hardlink, copy, or mutate the user's game assets.
        return {"source": str(source),
                "world_version": info["world_version"], "read_only": True}
    raise FileNotFoundError(
        f"No validated local Minecraft {version} client.jar found in gitignored worktree caches."
    )


def correlate_gpu_interference(report_paths, gpu_log):
    """Associate other-app GPU occupancy with actual scene flight windows.

    Whole-run GPU statistics exaggerate or conceal interference by mixing
    shader compilation and world loading with measured gameplay. These
    values are evidence of scheduling contention, not GPU quotas.
    """
    if not gpu_log.is_file():
        return [{"error": "GPU counter log missing"}]
    samples = []
    for line in gpu_log.read_text(encoding="utf-8").splitlines():
        try:
            row = json.loads(line)
            if isinstance(row.get("timestamp_unix"), (int, float)) and "other_gpu3d_percent" in row:
                samples.append(row)
        except ValueError:
            pass
    def nearest_rank(values, percentile):
        if not values:
            return None
        import math
        values.sort()
        return round(values[min(len(values)-1, math.ceil(len(values)*percentile)-1)], 3)
    results = []
    for report in report_paths:
        try:
            j = json.loads(Path(report).read_text(encoding="utf-8"))
            for frame in j.get("frames", []):
                window = frame.get("measurement_window_unix")
                if not window or len(window) != 2:
                    results.append({"report": str(report), "phase": frame.get("phase"),
                                    "error": "No measured flight time window"})
                    continue
                rows = [s for s in samples if window[0] <= s["timestamp_unix"] <= window[1]]
                external = [r["other_gpu3d_percent"] for r in rows]
                game = [r["horizon_gpu3d_percent"] for r in rows]
                compositor = [r["compositor_gpu3d_percent"] for r in rows]
                results.append({
                    "report": str(report),
                    "phase": frame.get("phase"),
                    "repeat": frame.get("repeat"),
                    "measurement_window_unix": window,
                    "sample_count": len(rows),
                    "other_gpu3d_p50": nearest_rank(external, 0.5),
                    "other_gpu3d_p95": nearest_rank(external, 0.95),
                    "other_gpu3d_max": nearest_rank(external, 1.0),
                    "horizon_gpu3d_p95": nearest_rank(game, 0.95),
                    "compositor_gpu3d_p95": nearest_rank(compositor, 0.95),
                    "note": "3D engine counters only; scheduling priority is not GPU isolation.",
                })
        except (OSError, ValueError, KeyError) as error:
            results.append({"report": str(report), "error": str(error)})
    return results


def main():
    p = argparse.ArgumentParser(description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--preset", choices=PRESETS, default="smoke")
    p.add_argument("--profile", choices=("fast", "release"), default="fast")
    p.add_argument("--build", type=Path, help="Directory containing optimized horizon-client.exe")
    p.add_argument("--resources", type=Path, help="Prepared 26.3 resource directory")
    p.add_argument("--cores", type=int, default=8)
    p.add_argument("--worker-threads", type=int, help="Override engine worker pool count; default matches allocated cores")
    p.add_argument("--gpu-background", choices=("below", "idle", "off"), default="below",
                   help="Temporary GPU priority for background apps; lower priority never reserves GPU bandwidth")
    p.add_argument("--backend", choices=("dx12", "vulkan"), default="dx12")
    p.add_argument("--label", help="Unique benchmark label; autogenerated if omitted")
    p.add_argument("--sizes", help="Override preset viewport(s), e.g. 1920x1080")
    p.add_argument("--distances", help="Override render distance(s), e.g. 8,16")
    p.add_argument("--scenes", help="Override scene(s), e.g. forest,village")
    p.add_argument("--seconds", type=float, help="Flight seconds per scene")
    p.add_argument("--repeats", type=int, help="Independent runs per condition")
    p.add_argument("--check", action="store_true",
                   help="Verify executable, assets and commands without starting anything")
    args = p.parse_args()
    if sys.platform != "win32" or platform.machine().upper() != "ARM64":
        p.error("Native Windows ARM64 required; do not benchmark under x86 emulation")
    if not 1 <= args.cores <= (os.cpu_count() or 2) - 1:
        p.error("Allocated Horizon cores must leave at least one core for the desktop")
    worker_threads = args.worker_threads if args.worker_threads is not None else args.cores
    if not 1 <= worker_threads <= 64:
        p.error("Worker thread count must be 1..64")
    cfg = dict(PRESETS[args.preset])
    for key in ("sizes", "distances", "scenes", "seconds", "repeats"):
        requested = getattr(args, key)
        if requested is not None:
            cfg[key] = requested
    try:
        sizes = [tuple(map(int, item.split("x"))) for item in cfg["sizes"].split(",")]
        distances = [int(d) for d in cfg["distances"].split(",")]
        assert sizes and all(320 <= w <= 7680 and 240 <= h <= 4320 for w, h in sizes)
        assert distances and all(2 <= d <= 64 for d in distances)
        assert cfg["seconds"] >= 2 and cfg["seconds"] <= 120
        assert 1 <= cfg["repeats"] <= 20
    except (ValueError, AssertionError):
        p.error("Invalid benchmark sizes, distances, repeat count or duration.")
    build = (args.build or (ROOT / "target" / "aarch64-pc-windows-msvc" / args.profile)).resolve()
    exe = build / "horizon-client.exe"
    if any("debug" in component.lower() for component in build.parts):
        p.error("Debug builds are not valid FPS baselines; select fast or release.")
    if not exe.exists():
        p.error(f"Optimized executable missing: {exe}")
    try:
        machine = pe_machine(exe)
    except (OSError, ValueError) as e:
        p.error(f"Invalid executable: {e}")
    if machine != 0xAA64:
        p.error(f"Expected ARM64 PE machine 0xAA64, got {machine:#x} for {exe}")
    resources = args.resources or next(
        (candidate for candidate in resources_candidate()
         if (candidate / "horizon-resources.json").exists()), None)
    if resources is None:
        p.error("No prepared Horizon resources found. Supply --resources.")
    resources = resources.resolve()
    if not (resources / "horizon-resources.json").exists():
        p.error(f"Resource manifest missing: {resources}")
    if not (ISOLATION.exists() and PERF.exists()):
        p.error("Required in-repository benchmark scripts are missing.")
    try:
        client_jar = prepare_benchmark_client_jar(resources, args.check)
    except (OSError, ValueError, KeyError) as exc:
        p.error(str(exc))
    stamp = time.strftime("%Y%m%d-%H%M%S")
    label = args.label or f"snapdragon-{args.profile}-{args.preset}-{stamp}"
    output_dir = ROOT / "artifacts" / "benchmarks" / label
    bench_command = [
        sys.executable, str(PERF),
        "--game", "horizon",
        "--build", str(build),
        "--resources", str(resources),
        "--client-jar", client_jar["source"],
        "--label", label,
        "--sizes", cfg["sizes"],
        "--distances", cfg["distances"],
        "--scenes", cfg["scenes"],
        "--seconds", str(cfg["seconds"]),
        "--repeats", str(cfg["repeats"]),
        "--offscreen", "--physical-pixels",
    ]
    command = [
        sys.executable, str(ISOLATION),
        "--horizon-cores", str(args.cores),
        "--gpu-background", args.gpu_background,
        "--timeout", str(cfg["timeout"]),
        "--report", str(output_dir / "isolation.json"),
        "--",
    ] + bench_command
    settings = {
        "schema": "horizon-snapdragon-gameplay-benchmark-v1",
        "processor": platform.processor(),
        "architecture": platform.machine(),
        "windows_version": platform.version(),
        "cpu_count": os.cpu_count(),
        "binary": str(exe),
        "binary_sha256": sha256(exe),
        "resources_manifest": str(resources / "horizon-resources.json"),
        "minecraft_client_version_cache": client_jar,
        "backend": args.backend,
        "profile": args.profile,
        "preset": args.preset,
        "horizon_cores": args.cores,
        "horizon_worker_threads": worker_threads,
        "background_gpu_priority": args.gpu_background,
        "benchmark": cfg,
        "label": label,
        "command": command,
        "use_offscreen": True,
        "note": "Offscreen results are controlled GPU workloads, not on-screen presentation latency.",
    }
    print(json.dumps(settings, indent=2), flush=True)
    if args.check:
        return 0
    # Named Windows mutex prevents multiple benchmark agents/worktrees from
    # simultaneously using the same GPU and contaminating FPS measurements.
    try:
        lease = BenchmarkLease()
    except (RuntimeError, OSError) as exc:
        p.error(str(exc))
    atexit.register(lease.close)
    prior_clients = running_clients()
    if prior_clients:
        p.error(
            f"Existing Horizon game process(es) {prior_clients} would contaminate "
            "the controlled benchmark; leave them running undisturbed."
        )
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "manifest.json").write_text(
        json.dumps(settings, indent=2) + "\n", encoding="utf-8")
    env = dict(os.environ)
    env["WGPU_BACKEND"] = args.backend
    env["HORIZON_WORKER_THREADS"] = str(worker_threads)
    # Never let two independent tests share the integrated-server world.
    env["HORIZON_PERF_WORK"] = str(output_dir / "game-work")
    for key in list(env):
        if key.startswith("HORIZON_TOUCH_PREVIEW"):
            del env[key]
    # Never alter personal settings or stop user applications. The isolation
    # controller restores the browser cores on all normal/crash paths.
    start = time.perf_counter()
    concurrent_events = []
    stop_monitor = threading.Event()

    def monitor_competing_clients():
        while not stop_monitor.wait(1.0):
            try:
                clients = running_clients()
                if len(clients) > 1:
                    concurrent_events.append({
                        "elapsed_seconds": round(time.perf_counter() - start, 3),
                        "client_pids": clients,
                    })
            except OSError as error:
                concurrent_events.append({"monitor_error": str(error)})
                return

    monitor = threading.Thread(target=monitor_competing_clients, daemon=True)
    monitor.start()
    try:
        with (output_dir / "run.log").open("w", encoding="utf-8") as log:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=log,
                                    stderr=subprocess.STDOUT)
    finally:
        stop_monitor.set()
        monitor.join(timeout=2)
    settings["exit_code"] = result.returncode
    settings["elapsed_seconds"] = round(time.perf_counter() - start, 3)
    settings["concurrent_horizon_detected"] = bool(concurrent_events)
    settings["concurrent_horizon_events"] = concurrent_events[:20]
    reports = []
    for size in cfg["sizes"].split(","):
        w, h = (int(a) for a in size.split("x"))
        suffix = "" if (w, h) == (1280, 720) else f"-{h}p" if w * 9 == h * 16 else f"-{w}x{h}"
        for distance in cfg["distances"].split(","):
            path = ROOT / "benchmarks" / "perf" / f"{label}{suffix}-rd{distance}.json"
            if path.exists():
                reports.append(str(path))
    settings["reports"] = reports
    # The physical render target is the workload, not the logical window size.
    # Reject DPI-mismatched reports rather than comparing incomparable FPS.
    size_errors = []
    for report_path in reports:
        try:
            doc = json.loads(Path(report_path).read_text(encoding="utf-8"))
            measured = doc.get("conditions", {}).get("window")
            # The filename encodes the requested resolution. Determine from
            # current benchmark configuration, not a filename suffix guess.
            name = Path(report_path).name
            requested = None
            for token in cfg["sizes"].split(","):
                width, height = (int(v) for v in token.split("x"))
                suffix = "" if (width, height) == (1280, 720) else (
                    f"-{height}p" if width * 9 == height * 16 else f"-{width}x{height}"
                )
                if any(name == f"{label}{suffix}-rd{distance}.json" for distance in cfg["distances"].split(",")):
                    requested = [width, height]
                    break
            if requested is None or measured != requested:
                size_errors.append({"report": str(report_path), "requested": requested, "rendered": measured})
        except (OSError, ValueError, KeyError) as err:
            size_errors.append({"report": str(report_path), "error": str(err)})
    settings["physical_render_resolution_verified"] = len(reports) > 0 and not size_errors
    settings["resolution_errors"] = size_errors
    settings["flight_gpu_interference"] = correlate_gpu_interference(
        reports, output_dir / "isolation.gpu.jsonl"
    )
    (output_dir / "manifest.json").write_text(
        json.dumps(settings, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({
        "exit_code": result.returncode,
        "elapsed_seconds": settings["elapsed_seconds"],
        "gameplay_reports": reports,
        "concurrent_horizon_detected": bool(concurrent_events),
        "isolation": str(output_dir / "isolation.json"),
        "gpu_samples": str(output_dir / "isolation.gpu.jsonl"),
        "run_log": str(output_dir / "run.log"),
    }, indent=2))
    if result.returncode or not reports:
        print("Horizon benchmark did not produce a verified gameplay report.",file=sys.stderr)
        return result.returncode or 2
    if size_errors:
        print("Physical render resolution does not match benchmark request: "
              + json.dumps(size_errors), file=sys.stderr)
        return 3
    if concurrent_events:
        print("A competing Horizon process invalidated this benchmark: "
              + json.dumps(concurrent_events[:5]), file=sys.stderr)
        return 4
    return 0


if __name__ == "__main__":
    sys.exit(main())
