"""100 BPS lore vs GHOSTDAG k. Not live telemetry."""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from bps100_cnf import (  # noqa: E402
    GHOSTDAG_K,
    LIVE_BPS,
    LORE_BPS,
    concurrent_blocks,
    k_covers,
    run_instance,
)


class BpsUnitsTests(unittest.TestCase):
    def test_lore_is_not_live(self) -> None:
        self.assertEqual(LIVE_BPS, 10)
        self.assertEqual(LORE_BPS, 100)
        self.assertEqual(GHOSTDAG_K, 18)
        self.assertNotEqual(LORE_BPS, LIVE_BPS)

    def test_k18_covers_10bps_not_100bps(self) -> None:
        self.assertTrue(k_covers(10, 18, 1))
        self.assertFalse(k_covers(100, 18, 1))
        self.assertTrue(k_covers(100, 100, 1))
        self.assertEqual(concurrent_blocks(100, 1), 100)


class BpsSolverTests(unittest.TestCase):
    def test_sweep_matches_arithmetic(self) -> None:
        out = Path(tempfile.mkdtemp(prefix="bps100_"))
        live = run_instance(LIVE_BPS, GHOSTDAG_K, 1, out)
        lore = run_instance(LORE_BPS, GHOSTDAG_K, 1, out)
        if live["solver"] == "no-pysat":
            self.skipTest("python-sat not installed")
        self.assertTrue(live["match"])
        self.assertEqual(live["arithmetic"], "SAT")
        self.assertTrue(lore["match"])
        self.assertEqual(lore["arithmetic"], "UNSAT")
        self.assertEqual(lore["encoded_n"], GHOSTDAG_K + 1)


if __name__ == "__main__":
    unittest.main()
