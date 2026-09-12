"""Offline tests for TN10 adoption scorecard recommendations."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from tn10_adoption_scorecard import (  # noqa: E402
    public_recommendations,
    recommendations,
)
from tn10_ibd_watch import STAGE_HEALTHY  # noqa: E402


class AdoptionRecommendationsTests(unittest.TestCase):
    def test_all_unreachable_suggests_start_kaspad(self) -> None:
        nodes = [{"name": "node1", "stage": "unreachable", "error": "refused"}]
        recs = recommendations(nodes, None, 2)
        self.assertTrue(any("tn10_node_onboard" in r for r in recs))

    def test_healthy_node_suggests_deposits(self) -> None:
        nodes = [{"name": "node1", "stage": STAGE_HEALTHY, "daa": 1, "gap": 0, "synced": True}]
        recs = recommendations(nodes, {"exitCode": 0}, 1)
        self.assertTrue(any("tn10-deposits" in r for r in recs))

    def test_public_unreachable_suggests_offline_fixtures(self) -> None:
        recs = public_recommendations({"stage": "unreachable", "url": "https://api-tn10.kaspa.org", "error": "timeout"})
        self.assertTrue(any("tn10-proof" in r for r in recs))

    def test_public_reachable_warns_no_deposits(self) -> None:
        recs = public_recommendations({"stage": "reachable", "url": "https://api-tn10.kaspa.org"})
        self.assertTrue(any("Do not credit deposits" in r for r in recs))
        self.assertTrue(any("1128" in r for r in recs))


if __name__ == "__main__":
    unittest.main()
