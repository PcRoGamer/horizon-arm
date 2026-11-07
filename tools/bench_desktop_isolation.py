#!/usr/bin/env python3
"""Temporarily partition a busy Windows ARM64 desktop for Horizon benchmarking.

Noncritical, user-owned background applications are moved to the last N cores.
Horizon and its children are launched in a Windows Job Object restricted to the
first N cores. The original background affinities are restored on completion.
An independent watchdog also restores them if the controller crashes.

Nothing is permanently installed or changed in Windows. Never modify system
processes, services, UniSync, or core parking. The GPU, OS/driver threads,
memory bandwidth, thermal state and some protected processes remain shared.

Typical example on the 12-core Snapdragon X Elite:
  python tools/bench_desktop_isolation.py --horizon-cores 8 \
      --report artifacts/isolated-run.json -- horizon-client.exe ...

Dry run to view eligible processes without modifying them:
  python tools/bench_desktop_isolation.py --dry-run

The bundled resource-envelope wrapper produces CPU and memory telemetry.
Actual FPS measurement comes from Horizon's benchmark instrumentation.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

TH32CS_SNAPPROCESS = 0x00000002
PROCESS_QUERY_INFORMATION = 0x0400
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
PROCESS_SET_INFORMATION = 0x0200
SYNCHRONIZE = 0x00100000
BELOW_NORMAL_PRIORITY_CLASS = 0x00004000
INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value
DEFAULT_BACKGROUND = "firefox.exe,chrome.exe,code.exe"
# Refuse core Windows processes even if explicitly requested by an operator.
NEVER_TOUCH = {
    "system", "registry", "idle", "smss.exe", "csrss.exe", "wininit.exe",
    "winlogon.exe", "lsass.exe", "services.exe", "svchost.exe",
    "dwm.exe", "explorer.exe", "taskhostw.exe", "sihost.exe", "fontdrvhost.exe",
    "securityhealthservice.exe", "msmpeng.exe", "unisync.exe",
    "python.exe", "pythonw.exe", "processgovernor.exe",
    "horizon-client.exe", "processlasso.exe",
}


class FILETIME(ctypes.Structure):
    _fields_ = [("low", w.DWORD), ("high", w.DWORD)]


class PROCESSENTRY32W(ctypes.Structure):
    _fields_ = [
        ("dwSize", w.DWORD), ("cntUsage", w.DWORD),
        ("th32ProcessID", w.DWORD), ("th32DefaultHeapID", ctypes.c_size_t),
        ("th32ModuleID", w.DWORD), ("cntThreads", w.DWORD),
        ("th32ParentProcessID", w.DWORD), ("pcPriClassBase", w.LONG),
        ("dwFlags", w.DWORD), ("szExeFile", w.WCHAR * 260),
    ]


def k32():
    k = ctypes.WinDLL("kernel32", use_last_error=True)
    k.CreateToolhelp32Snapshot.argtypes = [w.DWORD, w.DWORD]
    k.CreateToolhelp32Snapshot.restype = w.HANDLE
    k.Process32FirstW.argtypes = [w.HANDLE, ctypes.POINTER(PROCESSENTRY32W)]
    k.Process32FirstW.restype = w.BOOL
    k.Process32NextW.argtypes = [w.HANDLE, ctypes.POINTER(PROCESSENTRY32W)]
    k.Process32NextW.restype = w.BOOL
    k.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
    k.OpenProcess.restype = w.HANDLE
    k.CloseHandle.argtypes = [w.HANDLE]
    k.CloseHandle.restype = w.BOOL
    k.GetProcessAffinityMask.argtypes = [
        w.HANDLE, ctypes.POINTER(ctypes.c_size_t), ctypes.POINTER(ctypes.c_size_t)]
    k.GetProcessAffinityMask.restype = w.BOOL
    k.SetProcessAffinityMask.argtypes = [w.HANDLE, ctypes.c_size_t]
    k.SetProcessAffinityMask.restype = w.BOOL
    k.GetPriorityClass.argtypes = [w.HANDLE]
    k.GetPriorityClass.restype = w.DWORD
    k.SetPriorityClass.argtypes = [w.HANDLE, w.DWORD]
    k.SetPriorityClass.restype = w.BOOL
    k.GetProcessTimes.argtypes = [w.HANDLE] + [ctypes.POINTER(FILETIME)] * 4
    k.GetProcessTimes.restype = w.BOOL
    k.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
    k.WaitForSingleObject.restype = w.DWORD
    return k


def enumerate_processes(k):
    snap = k.CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
    if not snap or int(snap) == INVALID_HANDLE_VALUE:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        entry = PROCESSENTRY32W()
        entry.dwSize = ctypes.sizeof(entry)
        if k.Process32FirstW(snap, ctypes.byref(entry)):
            while True:
                yield int(entry.th32ProcessID), str(entry.szExeFile).lower()
                if not k.Process32NextW(snap, ctypes.byref(entry)):
                    break
    finally:
        k.CloseHandle(snap)


def process_details(k, h):
    affinity = ctypes.c_size_t(0)
    system = ctypes.c_size_t(0)
    if not k.GetProcessAffinityMask(h, ctypes.byref(affinity), ctypes.byref(system)):
        raise ctypes.WinError(ctypes.get_last_error())
    a, b, c, d = FILETIME(), FILETIME(), FILETIME(), FILETIME()
    if not k.GetProcessTimes(h, ctypes.byref(a), ctypes.byref(b), ctypes.byref(c), ctypes.byref(d)):
        raise ctypes.WinError(ctypes.get_last_error())
    born = (a.high << 32) | a.low
    priority = int(k.GetPriorityClass(h))
    if not priority:
        raise ctypes.WinError(ctypes.get_last_error())
    return int(affinity.value), born, priority


def open_process(k, pid):
    return k.OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SET_INFORMATION, False, pid)


def write_snapshot(path, payload):
    data = json.dumps(payload, indent=2)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(data + "\n", encoding="utf-8")
    os.replace(tmp, path)


def restore(k, records):
    outcomes = []
    for pidstr, info in records.items():
        pid = int(pidstr)
        h = open_process(k, pid)
        if not h:
            outcomes.append({"pid": pid, "status": "exited_or_unavailable"})
            continue
        try:
            affinity, born, priority = process_details(k, h)
            if born != info["created"]:
                outcomes.append({"pid": pid, "status": "pid_reused"})
                continue
            ok_affinity = bool(k.SetProcessAffinityMask(h, info["affinity"]))
            ok_priority = True
            if info["changed_priority"]:
                ok_priority = bool(k.SetPriorityClass(h, info["priority"]))
            outcomes.append({
                "pid": pid, "status": "restored" if ok_affinity and ok_priority else "restore_failed"
            })
        except OSError as e:
            outcomes.append({"pid": pid, "status": "restore_failed", "error": str(e)})
        finally:
            k.CloseHandle(h)
    return outcomes


def watchdog(snapshot, parent_pid):
    """Independent restorer: parent termination, including forced termination."""
    k = k32()
    parent = k.OpenProcess(SYNCHRONIZE, False, parent_pid)
    if not parent:
        # If the parent has already gone, restore from the last valid snapshot.
        pass
    try:
        while parent and k.WaitForSingleObject(parent, 700) == 0x102:
            try:
                doc = json.loads(snapshot.read_text(encoding="utf-8"))
                if doc.get("completed"):
                    return 0
            except (FileNotFoundError, ValueError, OSError):
                pass
        # Give parent a moment to write its final snapshot on normal exit.
        time.sleep(0.2)
        try:
            doc = json.loads(snapshot.read_text(encoding="utf-8"))
            if not doc.get("completed"):
                restore(k, doc.get("records", {}))
        except (FileNotFoundError, ValueError, OSError):
            pass
    finally:
        if parent:
            k.CloseHandle(parent)
    return 0


def partition(k, allow, background_mask, records, errors, snapshot):
    for pid, name in enumerate_processes(k):
        if name not in allow or name in NEVER_TOUCH or pid == os.getpid():
            continue
        if str(pid) in records:
            continue
        h = open_process(k, pid)
        if not h:
            errors.setdefault(f"{name}:{pid}", "Cannot open: protected or inaccessible")
            continue
        try:
            affinity, born, priority = process_details(k, h)
            desired = affinity & background_mask
            if not desired:
                errors.setdefault(f"{name}:{pid}", "Original affinity has no background cores")
                continue
            if affinity == desired:
                continue
            records[str(pid)] = {
                "name": name, "created": born,
                "affinity": affinity, "priority": priority,
                "changed_priority": False,
            }
            # Record must be durable BEFORE any process is modified.
            write_snapshot(snapshot, {"completed": False, "records": records})
            if not k.SetProcessAffinityMask(h, desired):
                records.pop(str(pid), None)
                write_snapshot(snapshot, {"completed": False, "records": records})
                errors.setdefault(f"{name}:{pid}",
                                  f"SetProcessAffinityMask failed: {ctypes.get_last_error()}")
        except OSError as e:
            errors.setdefault(f"{name}:{pid}", str(e))
        finally:
            k.CloseHandle(h)


def main():
    p = argparse.ArgumentParser(description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--horizon-cores", type=int, default=8)
    p.add_argument("--background", default=DEFAULT_BACKGROUND,
                   help="Comma-separated process names; system processes are always excluded")
    p.add_argument("--cpu-cap", type=float)
    p.add_argument("--memory-mb", type=int)
    p.add_argument("--timeout", type=float, default=120)
    p.add_argument("--report", type=Path)
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--watchdog", type=Path, help=argparse.SUPPRESS)
    p.add_argument("--parent-pid", type=int, help=argparse.SUPPRESS)
    p.add_argument("command", nargs=argparse.REMAINDER)
    args = p.parse_args()
    if os.name != "nt":
        p.error("Windows only")
    if args.watchdog:
        if args.parent_pid is None:
            p.error("missing watchdog parent")
        return watchdog(args.watchdog, args.parent_pid)
    cores = os.cpu_count() or 1
    if args.horizon_cores < 1 or args.horizon_cores >= cores:
        p.error(f"Expected horizon cores between 1 and {cores-1}")
    allow = {n.strip().lower() for n in args.background.split(",") if n.strip()}
    if any(n in NEVER_TOUCH for n in allow):
        p.error("Background list includes protected program names; refusing")
    k = k32()
    before = []
    for pid, name in enumerate_processes(k):
        if name in allow:
            before.append({"pid": pid, "name": name})
    if args.dry_run:
        print(json.dumps({"mode": "dry_run", "logical_cores": cores,
                          "horizon_cores": list(range(args.horizon_cores)),
                          "background_cores": list(range(args.horizon_cores, cores)),
                          "background_processes": before}, indent=2))
        return 0
    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        p.error("missing executable command after --")
    path = args.report or (Path("artifacts") / "isolated-benchmark.json")
    path.parent.mkdir(parents=True, exist_ok=True)
    snapshot = path.with_suffix(".recovery.json")
    records = {}
    errors = {}
    write_snapshot(snapshot, {"completed": False, "records": records})
    # A crash of the controller must not leave the user's browser pinned forever.
    guardian = subprocess.Popen(
        [sys.executable, str(Path(__file__).resolve()), "--watchdog",
         str(snapshot.resolve()), "--parent-pid", str(os.getpid())],
        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL, close_fds=True,
        creationflags=0x00000008 | 0x00000200)  # DETACHED_PROCESS | NEW_PROCESS_GROUP
    child = None
    exit_code = 1
    restoration = []
    started = time.monotonic()
    try:
        background_mask = ((1 << cores) - 1) ^ ((1 << args.horizon_cores) - 1)
        partition(k, allow, background_mask, records, errors, snapshot)
        wrapped = [
            sys.executable, str(Path(__file__).with_name("bench_resource_envelope.py")),
            "--affinity-count", str(args.horizon_cores),
            "--report", str(path.with_suffix(".envelope.json")),
        ]
        if args.cpu_cap is not None:
            wrapped += ["--cpu-cap", str(args.cpu_cap)]
        if args.memory_mb is not None:
            wrapped += ["--memory-mb", str(args.memory_mb)]
        wrapped += ["--"] + command
        child = subprocess.Popen(wrapped, creationflags=0x00000200)
        while child.poll() is None:
            if time.monotonic() - started > args.timeout:
                child.terminate()
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
                exit_code = 124
                break
            time.sleep(0.6)
            partition(k, allow, background_mask, records, errors, snapshot)
        else:
            exit_code = child.returncode
    finally:
        restoration = restore(k, records)
        write_snapshot(snapshot, {"completed": True, "records": records})
        # Don't wait on a background watchdog after normal restoration.
        report = {
            "schema": "horizon-desktop-isolation-v1",
            "horizon_cores": list(range(args.horizon_cores)),
            "background_cores": list(range(args.horizon_cores, cores)),
            "background_allowlist": sorted(allow),
            "observed_background_processes": before,
            "modified_count": len(records),
            "errors": errors,
            "restoration": restoration,
            "elapsed_seconds": round(time.monotonic() - started, 3),
            "exit_code": exit_code,
            "notes": [
                "GPU, interrupt load and kernel activity are not partitioned.",
                "A process that could not be modified remains a source of interference.",
                "Review the matching .envelope.json and Horizon frame-time report.",
            ],
        }
        path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(report, indent=2))
    return int(exit_code) if exit_code is not None else 1


if __name__ == "__main__":
    sys.exit(main())
