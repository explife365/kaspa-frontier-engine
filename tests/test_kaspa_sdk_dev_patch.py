"""Tests for TN10 SDK dev patch (PR #78 interim rehearsal)."""

from __future__ import annotations

import importlib
import os
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"


class DevPatchTests(unittest.TestCase):
    def test_dev_patch_fixes_compute_budget_probe(self) -> None:
        env = os.environ.copy()
        env["TN10_SDK_DEV_PATCH"] = "1"
        code = (
            "import sys; sys.path.insert(0, r'%s'); "
            "from kaspa_sdk_dev_patch import apply_dev_patch, probe_compute_budget; "
            "assert apply_dev_patch(); ok, msg, val = probe_compute_budget(); "
            "assert ok, msg; print(val)"
        ) % SCRIPTS
        proc = subprocess.run(
            [sys.executable, "-c", code],
            capture_output=True,
            text=True,
            cwd=ROOT,
            env=env,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr or proc.stdout)
        self.assertEqual(proc.stdout.strip(), "10")

    def test_gate_dev_mode_exits_zero_with_patch(self) -> None:
        env = os.environ.copy()
        env["TN10_SDK_DEV_PATCH"] = "1"
        proc = subprocess.run(
            [sys.executable, str(SCRIPTS / "tn10_sdk_gate.py"), "--dev", "--json"],
            capture_output=True,
            text=True,
            cwd=ROOT,
            env=env,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr or proc.stdout)
        data = __import__("json").loads(proc.stdout)
        self.assertTrue(data["readyWithDevPatch"])
        self.assertFalse(data["readyNative"])


if __name__ == "__main__":
    unittest.main()
