#!/usr/bin/env python3
"""Unit tests for native GPU contention attribution and input handling."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from bench_gpu_activity import summarize


class ActivityTests(unittest.TestCase):
    def test_does_not_double_count_multiple_3d_engines_for_same_pid(self):
        events = [
            ("pid_10_luid_1_eng_0_engtype_3D", 35.0),
            ("pid_10_luid_1_eng_1_engtype_3D", 26.0),
            ("pid_11_luid_1_eng_0_engtype_3D", 4.5),
            ("pid_12_luid_1_eng_0_engtype_3D", 8.0),
            ("pid_12_luid_1_eng_0_engtype_Copy", 95.0),
            ("pid_13_luid_1_eng_0_engtype_3D", 0.2),
        ]
        r = summarize(events, {10: "horizon-client.exe", 11: "dwm.exe",
                               12: "firefox.exe", 13: "guardian.exe"},
                      "horizon-client.exe")
        self.assertEqual(r["horizon_gpu3d_percent"], 35)
        self.assertEqual(r["compositor_gpu3d_percent"], 4.5)
        self.assertEqual(r["other_gpu3d_percent"], 8.2)

    def test_handles_absent_or_zero_samples(self):
        r = summarize([], {}, "horizon-client.exe")
        self.assertEqual(r["other_gpu3d_percent"], 0)
        self.assertEqual(r["horizon_gpu3d_percent"], 0)
        self.assertEqual(r["top_processes"], [])

    @unittest.skipUnless(sys.platform == "win32", "Windows PDH only")
    def test_native_pdh_short_capture(self):
        with tempfile.TemporaryDirectory() as temp:
            target = Path(temp) / "gpu.jsonl"
            p = subprocess.run(
                [sys.executable,
                 str(Path(__file__).with_name("bench_gpu_activity.py")),
                 "--report", str(target), "--duration", "1.6",
                 "--interval", "0.5"],
                capture_output=True, text=True, timeout=7)
            self.assertEqual(p.returncode, 0, p.stderr)
            import json
            rows = [json.loads(x) for x in target.read_text().splitlines()]
            self.assertGreaterEqual(len(rows), 1)
            self.assertTrue(all("other_gpu3d_percent" in r for r in rows))


if __name__ == "__main__":
    unittest.main()
