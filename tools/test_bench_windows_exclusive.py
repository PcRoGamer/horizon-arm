#!/usr/bin/env python3
"""Native Windows cross-process benchmark exclusion regression tests."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bench_windows_exclusive import BenchmarkLease, running_clients


@unittest.skipUnless(os.name == "nt", "Windows process lease")
class BenchmarkExclusionTests(unittest.TestCase):
    def test_unique_lease_acquires(self):
        with BenchmarkLease("Local\\HorizonBenchmarkTest_" + uuid.uuid4().hex):
            self.assertIsInstance(running_clients(), list)

    def test_competing_process_cannot_acquire_same_lease(self):
        name = "Local\\HorizonBenchmarkTest_" + uuid.uuid4().hex
        code = ("import sys,time;sys.path.insert(0,r'"
                + str(Path(__file__).resolve().parent) +
                "');from bench_windows_exclusive import BenchmarkLease;"
                "lock=BenchmarkLease(sys.argv[1]);print('ACQUIRED',flush=True);time.sleep(2.5)")
        proc = subprocess.Popen([sys.executable, "-c", code, name],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                text=True)
        try:
            self.assertEqual(proc.stdout.readline().strip(), "ACQUIRED")
            with self.assertRaisesRegex(RuntimeError, "Another controlled Horizon benchmark"):
                BenchmarkLease(name)
            proc.kill()
            proc.wait(timeout=3)
            # The OS releases a named mutex on forced termination.
            with BenchmarkLease(name):
                pass
        finally:
            if proc.poll() is None:
                proc.kill()
            proc.wait(timeout=3)
            if proc.stdout:
                proc.stdout.close()
            if proc.stderr:
                proc.stderr.close()


if __name__ == "__main__":
    unittest.main()
