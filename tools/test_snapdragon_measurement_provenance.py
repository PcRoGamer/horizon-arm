#!/usr/bin/env python3
"""Synthetic checks for Snapdragon benchmark measurement provenance."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import bench_snapdragon_arm as bench


class MeasurementProvenanceTests(unittest.TestCase):
    def test_gpu_interference_only_uses_measured_flight_interval(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            report = root / "report.json"
            report.write_text(json.dumps({
                "frames": [
                    {"phase": "forest", "repeat": 0,
                     "measurement_window_unix": [100, 106]},
                ],
            }), encoding="utf-8")
            gpu = root / "gpu.jsonl"
            readings = [
                {"timestamp_unix": 90, "other_gpu3d_percent": 99,
                 "horizon_gpu3d_percent": 0, "compositor_gpu3d_percent": 2},
                {"timestamp_unix": 101, "other_gpu3d_percent": 12,
                 "horizon_gpu3d_percent": 70, "compositor_gpu3d_percent": 2},
                {"timestamp_unix": 105, "other_gpu3d_percent": 30,
                 "horizon_gpu3d_percent": 80, "compositor_gpu3d_percent": 3},
                {"timestamp_unix": 110, "other_gpu3d_percent": 90,
                 "horizon_gpu3d_percent": 0, "compositor_gpu3d_percent": 2},
            ]
            gpu.write_text("".join(json.dumps(x) + "\n" for x in readings), encoding="utf-8")
            result = bench.correlate_gpu_interference([report], gpu)
            self.assertEqual(len(result), 1)
            self.assertEqual(result[0]["sample_count"], 2)
            self.assertEqual(result[0]["other_gpu3d_max"], 30)
            self.assertEqual(result[0]["other_gpu3d_p50"], 12)
            self.assertEqual(result[0]["horizon_gpu3d_p95"], 80)

    def test_pinned_client_jar_remains_read_only_for_check_and_run(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp) / "horizon-synthetic"
            repo.mkdir()
            (repo / "research-lock.json").write_text(
                json.dumps({"minecraft": {"version": "26.3"}}),
                encoding="utf-8")
            source = repo / ".local" / "downloads" / "26.3" / "client.jar"
            source.parent.mkdir(parents=True)
            with zipfile.ZipFile(source, "w") as z:
                z.writestr("version.json", json.dumps({
                    "id": "26.3", "world_version": 5023,
                }))
            resources = repo / ".local" / "horizon-client" / "26.3"
            resources.mkdir(parents=True)
            target = repo / ".local" / "minecraft" / "26.3" / "jars" / "client.jar"
            with mock.patch.object(bench, "ROOT", repo):
                dry = bench.prepare_benchmark_client_jar(resources, True)
                self.assertEqual(dry["world_version"], 5023)
                self.assertFalse(target.exists())
                prepared = bench.prepare_benchmark_client_jar(resources, False)
                self.assertFalse(target.exists(), "Benchmark must not copy game assets")
                self.assertTrue(prepared["read_only"])
                self.assertEqual(prepared["source"], str(source))
                self.assertEqual(prepared["world_version"], 5023)


if __name__ == "__main__":
    unittest.main()
