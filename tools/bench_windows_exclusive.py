#!/usr/bin/env python3
"""Cross-process exclusive lease for controlled Windows ARM Horizon benchmarks.

The operating system owns the named mutex: it is released if the runner
crashes. A new benchmark is rejected while any runner holds the lease.
Use a stable per-user Local name across all worktrees and agents.
"""
from __future__ import annotations

import ctypes
from ctypes import wintypes as w
import os

DEFAULT_LEASE = "Local\\HorizonARMControlledBenchmarkV1"
WAIT_OBJECT_0 = 0
WAIT_ABANDONED = 0x80
WAIT_TIMEOUT = 0x102
PROCESS_SNAPSHOT = 0x2
INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value


class Entry(ctypes.Structure):
    _fields_ = [
        ("size", w.DWORD), ("usage", w.DWORD), ("pid", w.DWORD),
        ("heap", ctypes.c_size_t), ("module_id", w.DWORD),
        ("threads", w.DWORD), ("parent_pid", w.DWORD),
        ("priority_base", w.LONG), ("flags", w.DWORD),
        ("name", w.WCHAR * 260),
    ]


def kernel():
    if os.name != "nt":
        raise OSError("Benchmark exclusion requires Windows")
    k = ctypes.WinDLL("kernel32", use_last_error=True)
    k.CreateMutexW.argtypes = [ctypes.c_void_p, w.BOOL, w.LPCWSTR]
    k.CreateMutexW.restype = w.HANDLE
    k.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
    k.WaitForSingleObject.restype = w.DWORD
    k.ReleaseMutex.argtypes = [w.HANDLE]
    k.ReleaseMutex.restype = w.BOOL
    k.CloseHandle.argtypes = [w.HANDLE]
    k.CloseHandle.restype = w.BOOL
    k.CreateToolhelp32Snapshot.argtypes = [w.DWORD, w.DWORD]
    k.CreateToolhelp32Snapshot.restype = w.HANDLE
    k.Process32FirstW.argtypes = [w.HANDLE, ctypes.POINTER(Entry)]
    k.Process32FirstW.restype = w.BOOL
    k.Process32NextW.argtypes = [w.HANDLE, ctypes.POINTER(Entry)]
    k.Process32NextW.restype = w.BOOL
    return k


class BenchmarkLease:
    def __init__(self, name=DEFAULT_LEASE):
        self.k = kernel()
        self.h = self.k.CreateMutexW(None, False, name)
        if not self.h:
            raise ctypes.WinError(ctypes.get_last_error())
        status = self.k.WaitForSingleObject(self.h, 0)
        if status not in (WAIT_OBJECT_0, WAIT_ABANDONED):
            self.k.CloseHandle(self.h)
            self.h = None
            if status == WAIT_TIMEOUT:
                raise RuntimeError(
                    "Another controlled Horizon benchmark is running. "
                    "Do not collect competing GPU samples on the same laptop."
                )
            raise RuntimeError(f"Windows benchmark mutex returned {status:#x}")

    def close(self):
        if self.h:
            self.k.ReleaseMutex(self.h)
            self.k.CloseHandle(self.h)
            self.h = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def running_clients():
    """Return all active Horizon client PIDs without changing any processes."""
    k = kernel()
    snap = k.CreateToolhelp32Snapshot(PROCESS_SNAPSHOT, 0)
    if not snap or int(snap) == INVALID_HANDLE_VALUE:
        raise ctypes.WinError(ctypes.get_last_error())
    found = []
    try:
        entry = Entry()
        entry.size = ctypes.sizeof(entry)
        if k.Process32FirstW(snap, ctypes.byref(entry)):
            while True:
                if entry.name.lower() == "horizon-client.exe":
                    found.append(int(entry.pid))
                if not k.Process32NextW(snap, ctypes.byref(entry)):
                    break
    finally:
        k.CloseHandle(snap)
    return sorted(set(found))
