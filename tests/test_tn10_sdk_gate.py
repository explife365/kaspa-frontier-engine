"""Tests for TN10 SDK Toccata gate."""

from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class SdkGateTests(unittest.TestCase):
    def test_sdk_gate_json_shape(self) -> None:
        proc = subprocess.run(
            [sys.executable, str(ROOT / "scripts" / "tn10_sdk_gate.py"), "--json"],
            capture_output=True,
            text=True,
            cwd=ROOT,
        )
        data = json.loads(proc.stdout)
        self.assertIn("ready", data)
        self.assertIn("computeBudgetOk", data)
        self.assertIn("silverscriptOk", data)
        self.assertIn("pr78Url", data)
        if not data["computeBudgetOk"]:
            self.assertEqual(proc.returncode, 1)


if __name__ == "__main__":
    unittest.main()
