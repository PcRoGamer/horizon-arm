#!/usr/bin/env python3
"""The Horizon benchmark control endpoint must match the engine's OS transport.

Modern Windows Python exposes AF_UNIX even though Horizon's Windows control
server always writes a TCP token/port JSON file. Keep this regression tested.
"""
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from unittest import TestCase, main, mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import perf_bench


class ControlPathTests(TestCase):
    def test_windows_uses_tcp_even_when_python_has_unix_socket(self):
        with mock.patch.object(perf_bench.sys, "platform", "win32"):
            p = perf_bench.control_path()
            self.assertEqual(p.suffix, ".json")
            self.assertNotEqual(p.parent, Path("/tmp"))

    def test_unix_uses_socket(self):
        with mock.patch.object(perf_bench.sys, "platform", "linux"):
            if hasattr(perf_bench.socket, "AF_UNIX"):
                self.assertEqual(perf_bench.control_path().suffix, ".sock")


    def test_independent_workdir_override(self):
        # Distinct concurrent runs must not share or erase the same world.
        with tempfile.TemporaryDirectory() as temp:
            env = dict(os.environ, HORIZON_PERF_WORK=temp)
            completed = subprocess.run(
                [sys.executable, "-c",
                 "import sys;sys.path.insert(0,'tools');import perf_bench;"
                 "print(perf_bench.WORK)"],
                cwd=perf_bench.ROOT,
                env=env,
                text=True, capture_output=True, timeout=8,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(Path(completed.stdout.strip()).resolve(), Path(temp).resolve())

    def test_windows_dpi_read_restores_awareness(self):
        if sys.platform != "win32":
            self.skipTest("Windows only")
        import ctypes
        user32 = ctypes.WinDLL("user32")
        user32.GetDpiForSystem.restype = ctypes.c_uint
        before = user32.GetDpiForSystem()
        scale = perf_bench.windows_dpi_scale_for_offscreen_benchmark()
        self.assertGreaterEqual(scale, 1.0)
        self.assertLessEqual(scale, 5.0)
        self.assertEqual(user32.GetDpiForSystem(), before)

if __name__ == "__main__":
    main()
