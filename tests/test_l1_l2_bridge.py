"""L1↔L2 HTLC bridge helpers (offline encoding)."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))

from l1_l2_bridge_release import encode_claim_demo, encode_deposit_wikas  # noqa: E402
from l1_l2_htlc_bridge import playbook  # noqa: E402


class L1L2BridgeTests(unittest.TestCase):
    def test_playbook_has_l1_and_l2(self) -> None:
        data = playbook()
        self.assertEqual(data["pattern"], "l1_htlc_lock_l2_preimage_release")
        self.assertIn("htlc-proof.json", data["l1"]["claim_fixture"])
        self.assertIn("wikas", data["l2"])

    def test_claim_demo_selector(self) -> None:
        data = encode_claim_demo(0x48544C43)
        self.assertTrue(data.startswith("0xee667c7b"))
        self.assertEqual(len(data), 2 + 8 + 64)

    def test_deposit_selector(self) -> None:
        data = encode_deposit_wikas(10**15)
        self.assertTrue(data.startswith("0xb6b55f25"))


if __name__ == "__main__":
    unittest.main()
