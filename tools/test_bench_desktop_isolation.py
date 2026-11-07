#!/usr/bin/env python3
"""Test Windows benchmark isolation and watchdog restoration without touching real apps."""
import ctypes
from ctypes import wintypes as w
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

SCRIPT = Path(__file__).with_name("bench_desktop_isolation.py")
TEST_EXE = "horizon_bench_background_probe.exe"
K = ctypes.WinDLL("kernel32", use_last_error=True) if os.name == "nt" else None
if K:
    K.GetProcessAffinityMask.argtypes = [w.HANDLE, ctypes.POINTER(ctypes.c_size_t), ctypes.POINTER(ctypes.c_size_t)]
    K.GetProcessAffinityMask.restype = w.BOOL


def affinity(proc):
    a, b = ctypes.c_size_t(), ctypes.c_size_t()
    if not K.GetProcessAffinityMask(proc._handle, ctypes.byref(a), ctypes.byref(b)):
        raise ctypes.WinError(ctypes.get_last_error())
    return a.value


def await_mask(proc, wanted, timeout=5):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        if proc.poll() is not None:
            raise AssertionError("Test process unexpectedly exited")
        if affinity(proc) == wanted:
            return
        time.sleep(0.1)
    raise AssertionError(f"Expected affinity {wanted:#x}, got {affinity(proc):#x}")


@unittest.skipUnless(os.name == "nt" and (os.cpu_count() or 0) >= 4, "Requires multicore Windows")
class DesktopPartitionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="horizon-bench-tests-")
        self.addCleanup(self.temp.cleanup)
        target = Path(self.temp.name) / TEST_EXE
        shutil.copyfile(Path(os.environ.get("SystemRoot", "C:\\Windows")) / "System32" / "ping.exe", target)
        self.bg = subprocess.Popen([str(target), "-n", "20", "127.0.0.1"],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.addCleanup(self.stop_bg)
        self.original = affinity(self.bg)
        self.horizon_cores = min(8, (os.cpu_count() or 4) - 2)
        self.background_mask = self.original & (
            ((1 << (os.cpu_count() or 4)) - 1) ^ ((1 << self.horizon_cores) - 1))
        self.assertNotEqual(self.original, self.background_mask)

    def stop_bg(self):
        if self.bg.poll() is None:
            self.bg.terminate()
        self.bg.wait(timeout=3)

    def controller(self, seconds):
        return subprocess.Popen([
            sys.executable, str(SCRIPT),
            "--horizon-cores", str(self.horizon_cores),
            "--background", TEST_EXE,
            "--timeout", "15",
            "--report", str(Path(self.temp.name) / "run.json"),
            "--", sys.executable, "-c", f"import time; time.sleep({seconds})",
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def test_normal_completion_restores_original_affinity(self):
        controller = self.controller(1.8)
        self.addCleanup(lambda: controller.kill() if controller.poll() is None else None)
        await_mask(self.bg, self.background_mask)
        self.assertEqual(controller.wait(timeout=6), 0)
        await_mask(self.bg, self.original)
        self.assertTrue((Path(self.temp.name) / "run.json").exists())

    def test_forced_termination_watchdog_recovers(self):
        controller = self.controller(8)
        self.addCleanup(lambda: controller.kill() if controller.poll() is None else None)
        await_mask(self.bg, self.background_mask)
        controller.kill()
        controller.wait(timeout=3)
        await_mask(self.bg, self.original)


if __name__ == "__main__":
    unittest.main()
