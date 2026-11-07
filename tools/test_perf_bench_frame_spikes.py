#!/usr/bin/env python3
"""Non-rendering regression tests for Horizon frame-time attribution."""
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parent))
from perf_bench import frames


class StutterAttributionTests(unittest.TestCase):
    def test_spikes_preserve_frame_order_and_cpu_gpu_samples(self):
        f = frames("forest", 0,
                   [5.0, 20.0, 100.0, 6.0],
                   [1.0, 2.0, 50.0, 1.0],
                   [4.0, 18.0, 10.0, 5.0])
        self.assertEqual(f["raw_frame_ms"], [5.0, 20.0, 100.0, 6.0])
        self.assertEqual(f["frames_over_16_67ms"], 2)
        worst = f["top_frame_spikes"][0]
        self.assertEqual(worst["frame_index"], 2)
        self.assertEqual(worst["frame_ms"], 100.0)
        self.assertEqual(worst["cpu_ms"], 50.0)
        self.assertEqual(worst["gpu_ms"], 10.0)
        self.assertEqual(worst["elapsed_s"], 0.125)

    def test_misaligned_gpu_samples_are_never_assigned_to_wrong_frames(self):
        f = frames("still", 0, [10.0, 30.0, 5.0], [1.0], [9.0, 28.0])
        self.assertEqual(f["frames_over_16_67ms"], 1)
        self.assertNotIn("cpu_ms", f["top_frame_spikes"][0])
        self.assertNotIn("gpu_ms", f["top_frame_spikes"][0])
        self.assertEqual(f["cpu_gpu_sample_alignment"]["frame_samples"], 3)


if __name__ == "__main__":
    unittest.main()
