"""Offline tests for integrator shim registry."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from integrator_shims import BLOCKERS, build_report, covenant_utxos_route, owned_node_urls  # noqa: E402


class IntegratorShimsTests(unittest.TestCase):
    def test_blocker_registry_has_seven_entries(self) -> None:
        self.assertEqual(len(BLOCKERS), 7)
        ids = {b.id for b in BLOCKERS}
        self.assertIn("covenant_utxo_index", ids)
        self.assertIn("covenant_sdk_broadcast", ids)
        self.assertIn("return_address_rpc", ids)
        self.assertIn("tx_input_enrichment", ids)

    def test_covenant_route_documents_kascov(self) -> None:
        route = covenant_utxos_route()
        self.assertTrue(route["shim_active"])
        self.assertIn("kascov.io", route["routes"]["today_indexer"])

    def test_owned_node_defaults(self) -> None:
        route = owned_node_urls()
        self.assertGreaterEqual(len(route["urls"]), 2)

    def test_build_report_shape(self) -> None:
        report = build_report()
        self.assertIn("blockers", report)
        self.assertIn("routes", report)
        self.assertTrue(report["not_consensus"])


if __name__ == "__main__":
    unittest.main()
