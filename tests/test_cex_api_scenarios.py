"""Offline + optional live tests for CEX/DEX API scenarios."""

from __future__ import annotations

import os
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))

from cex_api_common import (  # noqa: E402
    cex_readiness,
    cex_wallets,
    onboard_card,
    run_scenario,
    validate_scenarios,
)


class CexApiScenarioTests(unittest.TestCase):
    def test_onboard_nodes_card(self) -> None:
        card = onboard_card("nodes")
        self.assertTrue(card.get("ok"))
        self.assertEqual(card["id"], "nodes")
        self.assertIn("steps", card)

    def test_onboard_unknown_path_raises(self) -> None:
        with self.assertRaises(ValueError):
            onboard_card("missing")

    def test_cex_wallets_shape(self) -> None:
        body = cex_wallets(ROOT)
        self.assertEqual(body["network"], "testnet-10")
        self.assertTrue(body["not_consensus"])
        self.assertGreaterEqual(body["count"], 1)
        self.assertIn("address", body["wallets"][0])

    def test_cex_readiness_skip_gate(self) -> None:
        body = cex_readiness(skip_gate=True)
        self.assertEqual(body["network"], "testnet-10")
        self.assertIn("checks", body)
        self.assertIn("adoption_gate", body["checks"])

    def test_run_scenario_catches_errors(self) -> None:
        row = run_scenario("boom", lambda: (_ for _ in ()).throw(RuntimeError("x")))
        self.assertFalse(row["ok"])
        self.assertIn("x", row["body"]["error"])

    def test_validate_offline_bundle(self) -> None:
        report = validate_scenarios(skip_gate=True, live_dex=False)
        self.assertIn("scenarios", report)
        self.assertGreaterEqual(report["total"], 4)
        names = {s["scenario"] for s in report["scenarios"]}
        self.assertIn("fixtures_offline", names)
        self.assertIn("cex_wallets", names)
        self.assertIn("onboard_nodes", names)
        self.assertIn("cex_readiness", names)

    @unittest.skipUnless(
        os.environ.get("GALLEON_MINI_POOL") or os.environ.get("GALLEON_FEE_POOL"),
        "live DEX pool not configured",
    )
    def test_validate_live_dex_when_pool_set(self) -> None:
        report = validate_scenarios(skip_gate=True, live_dex=True)
        names = {s["scenario"] for s in report["scenarios"]}
        self.assertIn("dex_quote_gTEST_to_wiKAS", names)
        for row in report["scenarios"]:
            if row["scenario"].startswith("dex_"):
                self.assertTrue(row["ok"], row)

    def test_dex_pairs_mocked(self) -> None:
        fake_status = {
            "pool": "0xpool",
            "pair": {
                "token0": {"symbol": "gTEST"},
                "token1": {"symbol": "wiKAS"},
            },
            "reserves": {"human0": 1.0, "human1": 2.0},
            "price_token1_per_token0": 2.0,
            "fee_bps": 0,
        }
        with patch("galleon_dex_common.dex_status", return_value=fake_status), patch(
            "galleon_dex_common.pool_kind", return_value="mini_pool"
        ):
            from cex_api_common import dex_pairs

            body = dex_pairs()
            self.assertTrue(body["ok"])
            self.assertEqual(body["pairs"][0]["id"], "gTEST-wiKAS")


if __name__ == "__main__":
    unittest.main()
