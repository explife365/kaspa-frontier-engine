"""Offline tests for two-node sandbox report shape."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))

from tn10_two_node_sandbox import build_report  # noqa: E402
from tn10_ibd_watch import STAGE_HEALTHY  # noqa: E402


class TwoNodeSandboxTests(unittest.TestCase):
    def test_proof_ok_requires_gate_and_offline(self) -> None:
        gate = {
            "gateOk": True,
            "minHealthy": 2,
            "nodes": [
                {"name": "node1", "stage": STAGE_HEALTHY},
                {"name": "node2", "stage": STAGE_HEALTHY},
            ],
        }
        transfer = {
            "txid": "abc",
            "both_nodes_saw_utxo": True,
            "node_observations": [{"ok": True, "saw_utxo": True}, {"ok": True, "saw_utxo": True}],
        }
        report = build_report(gate=gate, transfer=transfer, covenant=None, offline={"ok": True})
        self.assertTrue(report["proof_ok"])

    def test_red_gate_fails_proof(self) -> None:
        gate = {"gateOk": False, "minHealthy": 2, "nodes": []}
        report = build_report(gate=gate, transfer=None, covenant=None, offline={"ok": True})
        self.assertFalse(report["proof_ok"])


if __name__ == "__main__":
    unittest.main()
