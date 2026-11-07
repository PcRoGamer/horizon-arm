#!/usr/bin/env python3
"""Preflight tests for native Snapdragon gameplay benchmarking (no game launch)."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bench_snapdragon_arm import pe_machine

SCRIPT = Path(__file__).with_name("bench_snapdragon_arm.py")


def write_pe(path, machine=0xAA64):
    path.parent.mkdir(parents=True, exist_ok=True)
    data = bytearray(256)
    data[:2] = b"MZ"
    data[0x3c:0x40] = (0x80).to_bytes(4, "little")
    data[0x80:0x84] = b"PE\x00\x00"
    data[0x84:0x86] = machine.to_bytes(2, "little")
    path.write_bytes(data)


class PreflightTests(unittest.TestCase):
    def test_architecture_detects_arm64_and_x64(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "horizon-client.exe"
            write_pe(p, 0xAA64)
            self.assertEqual(pe_machine(p), 0xAA64)
            write_pe(p, 0x8664)
            self.assertEqual(pe_machine(p), 0x8664)

    @unittest.skipUnless(sys.platform == "win32", "Windows ARM64 only")
    def test_preflight_writes_no_artifacts_and_accepts_arm64(self):
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            write_pe(tmp / "fast" / "horizon-client.exe")
            (tmp / "resources").mkdir()
            (tmp / "resources" / "horizon-resources.json").write_text("{}")
            cmd = [sys.executable, str(SCRIPT), "--check",
                   "--build", str(tmp / "fast"),
                   "--resources", str(tmp / "resources")]
            r = subprocess.run(cmd, capture_output=True, text=True, timeout=5)
            if sys.platform == "win32":
                import platform
                if platform.machine().upper() != "ARM64":
                    self.assertNotEqual(r.returncode, 0)
                    return
            self.assertEqual(r.returncode, 0, r.stderr)
            conf = json.loads(r.stdout)
            self.assertEqual(conf["architecture"].upper(), "ARM64")
            self.assertTrue(conf["use_offscreen"])
            self.assertEqual(conf["horizon_cores"], 8)
            self.assertEqual(conf["backend"], "dx12")
            self.assertEqual(conf["horizon_worker_threads"], 8)
            self.assertEqual(conf["background_gpu_priority"], "below")
            self.assertIn("--gpu-background", conf["command"])
            self.assertFalse((tmp / "benchmarks").exists())
            override = subprocess.run(
                cmd + ["--sizes", "1920x1080", "--distances", "8",
                       "--scenes", "forest", "--repeats", "2",
                       "--gpu-background", "idle", "--worker-threads", "6"],
                capture_output=True, text=True, timeout=7)
            self.assertEqual(override.returncode, 0, override.stderr)
            adjusted = json.loads(override.stdout)["benchmark"]
            self.assertEqual(adjusted["sizes"], "1920x1080")
            self.assertEqual(adjusted["repeats"], 2)
            self.assertEqual(json.loads(override.stdout)["horizon_worker_threads"], 6)
            self.assertEqual(json.loads(override.stdout)["background_gpu_priority"], "idle")
            invalid = subprocess.run(
                cmd + ["--sizes", "1920x1080", "--repeats", "0"],
                capture_output=True, text=True, timeout=7)
            self.assertNotEqual(invalid.returncode, 0)

    @unittest.skipUnless(sys.platform == "win32", "Windows ARM64 only")
    def test_debug_binary_cannot_be_used_as_performance_baseline(self):
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            write_pe(tmp / "debug" / "horizon-client.exe")
            r = subprocess.run([sys.executable, str(SCRIPT), "--check",
                                "--build", str(tmp / "debug")],
                               capture_output=True, text=True, timeout=5)
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("Debug builds are not valid", r.stderr)


if __name__ == "__main__":
    unittest.main()
