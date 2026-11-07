#!/usr/bin/env python3
"""Windows ARM64 Horizon benchmark resource envelope.

Runs a child command in a Windows Job Object with an optional CPU hard cap,
processor affinity, and committed-memory ceiling. Captures elapsed time,
per-process CPU use, peak RSS and machine-wide CPU busy time. No system-wide
settings or other processes are modified.

Example:
  python tools/bench_resource_envelope.py --cpu-cap 80 --affinity-count 10 --report artifacts/envelope.json -- horizon-client.exe --resources ...
  python tools/bench_resource_envelope.py --cpu-cap 5 -- python -c "..."
Use comparable limits on every version under test. For real gameplay FPS use
Horizon frame telemetry; CPU usage alone is not a frame benchmark.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time


class FILETIME(ctypes.Structure):
    _fields_ = [("low", wintypes.DWORD), ("high", wintypes.DWORD)]


class BasicLimit(ctypes.Structure):
    _fields_ = [
        ("per_process_user_time", ctypes.c_int64),
        ("per_job_user_time", ctypes.c_int64),
        ("limit_flags", wintypes.DWORD),
        ("min_working_set", ctypes.c_size_t),
        ("max_working_set", ctypes.c_size_t),
        ("active_process_limit", wintypes.DWORD),
        ("affinity", ctypes.c_size_t),
        ("priority_class", wintypes.DWORD),
        ("scheduling_class", wintypes.DWORD),
    ]


class IOCounters(ctypes.Structure):
    _fields_ = [(name, ctypes.c_uint64) for name in (
        "read_ops", "write_ops", "other_ops",
        "read_bytes", "write_bytes", "other_bytes",
    )]


class ExtendedLimit(ctypes.Structure):
    _fields_ = [
        ("basic", BasicLimit),
        ("io", IOCounters),
        ("process_memory_limit", ctypes.c_size_t),
        ("job_memory_limit", ctypes.c_size_t),
        ("peak_process_memory", ctypes.c_size_t),
        ("peak_job_memory", ctypes.c_size_t),
    ]


class CpuRate(ctypes.Structure):
    _fields_ = [("flags", wintypes.DWORD), ("rate", wintypes.DWORD)]


class ProcessMemoryCounters(ctypes.Structure):
    _fields_ = [
        ("cb", wintypes.DWORD),
        ("page_fault_count", wintypes.DWORD),
        ("peak_working_set", ctypes.c_size_t),
        ("working_set", ctypes.c_size_t),
        ("peak_paged_pool", ctypes.c_size_t),
        ("paged_pool", ctypes.c_size_t),
        ("peak_nonpaged_pool", ctypes.c_size_t),
        ("nonpaged_pool", ctypes.c_size_t),
        ("pagefile", ctypes.c_size_t),
        ("peak_pagefile", ctypes.c_size_t),
    ]


def ticks(f: FILETIME) -> int:
    return (f.high << 32) | f.low


def error(action: str) -> OSError:
    return ctypes.WinError(ctypes.get_last_error())


def native():
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateJobObjectW.argtypes = [ctypes.c_void_p, wintypes.LPCWSTR]
    kernel.CreateJobObjectW.restype = wintypes.HANDLE
    kernel.SetInformationJobObject.argtypes = [
        wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD]
    kernel.SetInformationJobObject.restype = wintypes.BOOL
    kernel.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
    kernel.AssignProcessToJobObject.restype = wintypes.BOOL
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel.CloseHandle.restype = wintypes.BOOL
    kernel.GetSystemTimes.argtypes = [ctypes.POINTER(FILETIME)] * 3
    kernel.GetSystemTimes.restype = wintypes.BOOL
    kernel.GetProcessTimes.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(FILETIME)] * 4
    kernel.GetProcessTimes.restype = wintypes.BOOL
    kernel.K32GetProcessMemoryInfo.argtypes = [
        wintypes.HANDLE, ctypes.POINTER(ProcessMemoryCounters), wintypes.DWORD]
    kernel.K32GetProcessMemoryInfo.restype = wintypes.BOOL
    return kernel


def system_ticks(k):
    idle, kernel, user = FILETIME(), FILETIME(), FILETIME()
    if not k.GetSystemTimes(ctypes.byref(idle), ctypes.byref(kernel), ctypes.byref(user)):
        raise error("GetSystemTimes")
    return (ticks(idle), ticks(kernel), ticks(user))


def process_cpu_seconds(k, h) -> float | None:
    a, b, c, d = FILETIME(), FILETIME(), FILETIME(), FILETIME()
    if not k.GetProcessTimes(h, ctypes.byref(a), ctypes.byref(b), ctypes.byref(c), ctypes.byref(d)):
        return None
    return (ticks(c) + ticks(d)) / 10_000_000


def peak_rss_bytes(k, h) -> int | None:
    v = ProcessMemoryCounters()
    v.cb = ctypes.sizeof(v)
    if not k.K32GetProcessMemoryInfo(h, ctypes.byref(v), v.cb):
        return None
    return v.peak_working_set


def set_information(k, job, kind: int, v) -> None:
    if not k.SetInformationJobObject(job, kind, ctypes.byref(v), ctypes.sizeof(v)):
        raise error("SetInformationJobObject")


def start_job(k, cpu_cap: float | None, affinity_count: int | None,
              memory_mb: int | None):
    job = k.CreateJobObjectW(None, None)
    if not job:
        raise error("CreateJobObjectW")
    try:
        extended = ExtendedLimit()
        extended.basic.limit_flags = 0x2000  # KILL_ON_JOB_CLOSE
        if affinity_count is not None:
            extended.basic.limit_flags |= 0x10  # AFFINITY
            extended.basic.affinity = (1 << affinity_count) - 1
        if memory_mb is not None:
            extended.basic.limit_flags |= 0x200  # JOB_MEMORY
            extended.job_memory_limit = memory_mb * 1024 * 1024
        set_information(k, job, 9, extended)  # JobObjectExtendedLimitInformation
        if cpu_cap is not None:
            # 10,000 = 100% of all logical-processor cycles, NOT one core.
            rate = CpuRate(0x1 | 0x4, round(cpu_cap * 100))  # ENABLE | HARD_CAP
            set_information(k, job, 15, rate)  # JobObjectCpuRateControlInformation
        return job
    except Exception:
        k.CloseHandle(job)
        raise


def run(args) -> int:
    if os.name != "nt":
        raise SystemExit("Windows Job Object resource envelopes require Windows.")
    ncpu = os.cpu_count() or 1
    if args.affinity_count is not None and not (1 <= args.affinity_count <= ncpu):
        raise SystemExit(f"Affinity count must be in 1..{ncpu}.")
    if args.cpu_cap is not None and not (0.01 <= args.cpu_cap <= 100):
        raise SystemExit("CPU cap must be 0.01..100 percent.")
    if args.memory_mb is not None and args.memory_mb < 64:
        raise SystemExit("Memory limit must be at least 64 MiB.")
    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        raise SystemExit("Expected command after --.")

    k = native()
    job = start_job(k, args.cpu_cap, args.affinity_count, args.memory_mb)
    child = None
    before = system_ticks(k)
    started = time.perf_counter()
    exit_code = None
    timed_out = False
    cpu_seconds = None
    rss_bytes = None
    try:
        child = subprocess.Popen(command, cwd=args.cwd or None)
        if not k.AssignProcessToJobObject(job, wintypes.HANDLE(child._handle)):
            child.kill()
            child.wait()
            raise error("AssignProcessToJobObject")
        try:
            exit_code = child.wait(timeout=args.timeout if args.timeout else None)
        except subprocess.TimeoutExpired:
            timed_out = True
            child.kill()
            exit_code = child.wait()
        cpu_seconds = process_cpu_seconds(k, wintypes.HANDLE(child._handle))
        rss_bytes = peak_rss_bytes(k, wintypes.HANDLE(child._handle))
    finally:
        elapsed = time.perf_counter() - started
        after = system_ticks(k)
        k.CloseHandle(job)  # Also cleans up children of the benchmarked process.
    total = (after[1] + after[2]) - (before[1] + before[2])
    busy = total - (after[0] - before[0])
    cpu_busy_percent = 100 * busy / total if total > 0 else None
    report = {
        "schema": "horizon-resource-envelope-v1",
        "machine": platform.node(),
        "architecture": platform.machine(),
        "logical_cpus": ncpu,
        "command": command,
        "cpu_cap_percent_total": args.cpu_cap,
        "affinity_count": args.affinity_count,
        "job_memory_cap_mib": args.memory_mb,
        "wall_seconds": round(elapsed, 4),
        "child_cpu_seconds": round(cpu_seconds, 4) if cpu_seconds is not None else None,
        "child_cpu_percent_machine": round(100 * cpu_seconds / (elapsed * ncpu), 3)
        if cpu_seconds is not None and elapsed > 0 else None,
        "machine_cpu_busy_percent": round(cpu_busy_percent, 3) if cpu_busy_percent is not None else None,
        "child_peak_rss_mib": round(rss_bytes / 1024**2, 3)
        if rss_bytes is not None else None,
        "timed_out": timed_out,
        "exit_code": exit_code,
        "note": "CPU cap is a maximum, not a reservation. Includes child process only in CPU/RSS values; job cap includes descendants. FPS requires game frame-time telemetry.",
    }
    encoded = json.dumps(report, indent=2)
    print(encoded)
    if args.report:
        path = Path(args.report)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(encoded + "\n", encoding="utf-8")
    return 124 if timed_out else (exit_code if exit_code is not None else 1)


def main():
    p = argparse.ArgumentParser(description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--cpu-cap", type=float, default=None, metavar="PERCENT")
    p.add_argument("--affinity-count", type=int, default=None, metavar="CORES")
    p.add_argument("--memory-mb", type=int, default=None, metavar="MIB")
    p.add_argument("--timeout", type=float, default=0, metavar="SECONDS")
    p.add_argument("--cwd", default=None)
    p.add_argument("--report", default=None)
    p.add_argument("command", nargs=argparse.REMAINDER)
    return run(p.parse_args())


if __name__ == "__main__":
    sys.exit(main())
