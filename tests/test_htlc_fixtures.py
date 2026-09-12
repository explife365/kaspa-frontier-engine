"""Checked-in HTLC claim + refund proof bundles validate offline."""

from __future__ import annotations

import json
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = (
    "fixtures/tn10-htlc-proof.json",
    "fixtures/tn10-htlc-refund-proof.json",
)


class HtlcFixtureTests(unittest.TestCase):
    def test_offline_proof_verifies(self) -> None:
        for rel in FIXTURES:
            path = ROOT / rel
            self.assertTrue(path.is_file(), rel)
            proc = subprocess.run(
                [
                    "cargo",
                    "run",
                    "--quiet",
                    "--release",
                    "--bin",
                    "tn10-proof",
                    "--",
                    str(path),
                    "--offline",
                    "--json",
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                timeout=120,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr or proc.stdout)
            report = json.loads(proc.stdout)
            self.assertTrue(report.get("complete"))


if __name__ == "__main__":
    unittest.main()
