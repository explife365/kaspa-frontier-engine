"""Delay-window occupancy sim. Not kaspad. Not DAGKnight."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from ghostdag_k_window_sim import (  # noqa: E402
    GHOSTDAG_K,
    LIVE_BPS,
    LORE_BPS,
    simulate,
)


class KWindowSimTests(unittest.TestCase):
    def test_live_fits_k_lore_does_not(self) -> None:
        live = simulate(bps=LIVE_BPS, miners=4, seconds=8.0, latency_s=1.0, k=GHOSTDAG_K, seed=10)
        lore = simulate(bps=LORE_BPS, miners=4, seconds=8.0, latency_s=1.0, k=GHOSTDAG_K, seed=11)
        self.assertTrue(live.covers)
        self.assertLessEqual(live.max_occupancy, GHOSTDAG_K)
        self.assertFalse(lore.covers)
        self.assertGreater(lore.max_occupancy, GHOSTDAG_K)
        self.assertIn("not DAGKnight", live.note)

    def test_not_a_protocol_label(self) -> None:
        report = simulate(bps=LIVE_BPS, seed=1)
        self.assertNotIn("activated", report.note.lower())

    def test_100bps_covers_only_when_k_scales(self) -> None:
        overflow = simulate(
            bps=LORE_BPS, miners=4, seconds=8.0, latency_s=1.0, k=GHOSTDAG_K, seed=11
        )
        at_bps = simulate(
            bps=LORE_BPS, miners=4, seconds=8.0, latency_s=1.0, k=LORE_BPS, seed=11
        )
        scaled = simulate(
            bps=LORE_BPS, miners=4, seconds=8.0, latency_s=1.0, k=128, seed=11
        )
        self.assertFalse(overflow.covers)
        self.assertFalse(at_bps.covers)
        self.assertGreater(at_bps.max_occupancy, LORE_BPS)
        self.assertTrue(scaled.covers)
        self.assertLessEqual(scaled.max_occupancy, 128)


if __name__ == "__main__":
    unittest.main()
