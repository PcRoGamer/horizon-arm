#!/usr/bin/env python3
"""Low-overhead Windows GPU contention recorder for Horizon ARM benchmarks.

Uses native Windows PDH (no PowerShell polling, external package, or driver
access). Samples 3D engine usage, separating Horizon, desktop composition,
and other apps. Writes one JSONL record per sample, immediately flushed.
A crash can lose at most one sample. Values are diagnostic utilization,
not additive fractions of physically reserved GPU throughput.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import re
import sys
import time

PATH = chr(92) + "GPU Engine(*)" + chr(92) + "Utilization Percentage"
PDH_FMT_DOUBLE = 0x00000200
PDH_MORE_DATA = 0x800007D2
INVALID_HANDLE = ctypes.c_void_p(-1).value
TH32CS_SNAPPROCESS = 0x2


class Value(ctypes.Structure):
    _fields_ = [("status", w.DWORD), ("double_value", ctypes.c_double)]


class Item(ctypes.Structure):
    _fields_ = [("name", w.LPWSTR), ("value", Value)]


class ProcessEntry(ctypes.Structure):
    _fields_ = [
        ("size", w.DWORD), ("usage", w.DWORD), ("pid", w.DWORD),
        ("heap", ctypes.c_size_t), ("module_id", w.DWORD), ("threads", w.DWORD),
        ("parent_pid", w.DWORD), ("priority_base", w.LONG), ("flags", w.DWORD),
        ("name", w.WCHAR * 260),
    ]


def process_names(kernel):
    h = kernel.CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
    if not h or int(h) == INVALID_HANDLE:
        return {}
    names = {}
    try:
        p = ProcessEntry()
        p.size = ctypes.sizeof(p)
        if kernel.Process32FirstW(h, ctypes.byref(p)):
            while True:
                names[int(p.pid)] = p.name.lower()
                if not kernel.Process32NextW(h, ctypes.byref(p)):
                    break
    finally:
        kernel.CloseHandle(h)
    return names


def pdh_init():
    p = ctypes.WinDLL("pdh", use_last_error=True)
    p.PdhOpenQueryW.argtypes = [w.LPCWSTR, ctypes.c_size_t, ctypes.POINTER(ctypes.c_void_p)]
    p.PdhAddEnglishCounterW.argtypes = [
        ctypes.c_void_p, w.LPCWSTR, ctypes.c_size_t, ctypes.POINTER(ctypes.c_void_p)]
    p.PdhCollectQueryData.argtypes = [ctypes.c_void_p]
    p.PdhGetFormattedCounterArrayW.argtypes = [
        ctypes.c_void_p, w.DWORD, ctypes.POINTER(w.DWORD),
        ctypes.POINTER(w.DWORD), ctypes.c_void_p]
    p.PdhCloseQuery.argtypes = [ctypes.c_void_p]
    query, counter = ctypes.c_void_p(), ctypes.c_void_p()
    a = p.PdhOpenQueryW(None, 0, ctypes.byref(query))
    if a:
        raise RuntimeError(f"PDH query failed: {a & 0xffffffff:#x}")
    b = p.PdhAddEnglishCounterW(query, PATH, 0, ctypes.byref(counter))
    if b:
        p.PdhCloseQuery(query)
        raise RuntimeError(f"PDH GPU Engine counter unavailable: {b & 0xffffffff:#x}")
    return p, query, counter


def read_counter(pdh, query, counter):
    a = pdh.PdhCollectQueryData(query)
    if a:
        raise RuntimeError(f"PDH sample failure: {a & 0xffffffff:#x}")
    size, count = w.DWORD(), w.DWORD()
    status = pdh.PdhGetFormattedCounterArrayW(
        counter, PDH_FMT_DOUBLE, ctypes.byref(size), ctypes.byref(count), None)
    if (status & 0xffffffff) != PDH_MORE_DATA or size.value > 16_000_000:
        raise RuntimeError(f"PDH counter array error: {status & 0xffffffff:#x}")
    data = ctypes.create_string_buffer(size.value)
    status = pdh.PdhGetFormattedCounterArrayW(
        counter, PDH_FMT_DOUBLE, ctypes.byref(size), ctypes.byref(count), data)
    if status:
        raise RuntimeError(f"PDH counter decode error: {status & 0xffffffff:#x}")
    items = ctypes.cast(data, ctypes.POINTER(Item))
    return [
        (items[i].name, float(items[i].value.double_value))
        for i in range(count.value)
        if items[i].value.status == 0 and items[i].value.double_value >= 0
    ]


def summarize(values, names, game_image):
    by_pid = {}
    for instance, utilization in values:
        # The 3D engine is the most comparable indicator across sessions;
        # counting every engine would double-count GPU work.
        if "engtype_3d" not in instance.lower():
            continue
        match = re.search(r"(?:^|_)pid_(\d+)(?:_|$)", instance.lower())
        if not match:
            continue
        pid = int(match.group(1))
        by_pid[pid] = max(by_pid.get(pid, 0.0), utilization)
    horizon, compositor, desktop = 0.0, 0.0, 0.0
    contributors = []
    for pid, percent in by_pid.items():
        image = names.get(pid, "unknown")
        if image == game_image:
            horizon += percent
        elif image == "dwm.exe":
            compositor += percent
        else:
            desktop += percent
        if percent >= 0.05:
            contributors.append({"pid": pid, "image": image, "gpu3d_percent": round(percent, 3)})
    contributors.sort(key=lambda x: -x["gpu3d_percent"])
    return {
        "horizon_gpu3d_percent": round(horizon, 3),
        "compositor_gpu3d_percent": round(compositor, 3),
        "other_gpu3d_percent": round(desktop, 3),
        "top_processes": contributors[:8],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", required=True, type=Path)
    parser.add_argument("--interval", type=float, default=1.0)
    parser.add_argument("--duration", type=float, default=0.0)
    parser.add_argument("--game-image", default="horizon-client.exe")
    parser.add_argument("--parent-pid", type=int, default=0)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Windows only")
    if not 0.2 <= args.interval <= 30:
        parser.error("Interval must be between 0.2 and 30 seconds")
    k = ctypes.WinDLL("kernel32", use_last_error=True)
    k.CreateToolhelp32Snapshot.argtypes = [w.DWORD, w.DWORD]
    k.CreateToolhelp32Snapshot.restype = w.HANDLE
    k.Process32FirstW.argtypes = [w.HANDLE, ctypes.POINTER(ProcessEntry)]
    k.Process32NextW.argtypes = [w.HANDLE, ctypes.POINTER(ProcessEntry)]
    k.CloseHandle.argtypes = [w.HANDLE]
    p, query, counter = pdh_init()
    # Guard against a killed benchmark controller leaving a long-lived monitor.
    k.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
    k.OpenProcess.restype = w.HANDLE
    k.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
    k.WaitForSingleObject.restype = w.DWORD
    parent = k.OpenProcess(0x00100000, False, args.parent_pid) if args.parent_pid else None
    args.report.parent.mkdir(parents=True, exist_ok=True)
    start = time.monotonic()
    count = 0
    try:
        with args.report.open("w", encoding="utf-8", buffering=1) as f:
            # GPU utilization is a rate counter and needs a previous sample.
            # Priming avoids invalid-data errors on the first formatted read.
            p.PdhCollectQueryData(query)
            time.sleep(min(args.interval, 0.5))
            while not args.duration or time.monotonic() - start < args.duration:
                if parent and k.WaitForSingleObject(parent, 0) != 0x102:
                    break
                time.sleep(args.interval)
                try:
                    record = summarize(
                        read_counter(p, query, counter),
                        process_names(k),
                        args.game_image.lower(),
                    )
                    record.update({"timestamp_unix": time.time(), "sample_index": count})
                    f.write(json.dumps(record, separators=(",", ":")) + "\n")
                    count += 1
                except Exception as e:
                    f.write(json.dumps({"timestamp_unix": time.time(), "error": str(e)}) + "\n")
                    return 2
    finally:
        p.PdhCloseQuery(query)
        if parent:
            k.CloseHandle(parent)
    print(json.dumps({"samples": count, "report": str(args.report)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
