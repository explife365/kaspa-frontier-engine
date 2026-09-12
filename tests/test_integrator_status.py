"""Integrator status and timeout playbook (offline)."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))

from integrator_status import FIXTURES, timeout_playbook, verify_fixture_offline  # noqa: E402
from l1_l2_bridge_timeout_playbook import playbook as timeout_pb  # noqa: E402
from l1_l2_htlc_bridge import playbook  # noqa: E402


class IntegratorStatusTests(unittest.TestCase):
    def test_playbook_has_integrator_status(self) -> None:
        data = playbook()
        self.assertIn("integrator_status", data)
        self.assertEqual(data["sha256_variant"]["status"], "l1_l2_claim_shipped")

    def test_timeout_playbook_int_refund_published(self) -> None:
        data = timeout_pb()
        self.assertTrue(data["int_tag"]["published"])
        self.assertIn("refund-rehearsal", data["sha256"]["broadcast_refund"])

    def test_timeout_playbook_keys(self) -> None:
        tp = timeout_playbook()
        self.assertIn("int_l1_verify_refund", tp)
        self.assertIn("sha256_l1_refund_command", tp)

    def test_all_shipped_fixtures_verify_offline(self) -> None:
        for name in FIXTURES:
            result = verify_fixture_offline(name)
            self.assertTrue(result.get("ok"), f"{name}: {result}")


if __name__ == "__main__":
    unittest.main()
