#!/usr/bin/env python3
"""The Horizon benchmark control endpoint must match the engine's OS transport.

Modern Windows Python exposes AF_UNIX even though Horizon's Windows control
server always writes a TCP token/port JSON file. Keep this regression tested.
"""
import sys
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


if __name__ == "__main__":
    main()
