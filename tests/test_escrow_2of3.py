"""Escrow 2-of-3 SilverScript helpers (offline)."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples" / "silverscript"))

import escrow_2of3  # noqa: E402


class Escrow2of3Tests(unittest.TestCase):
    def test_unlock_release_seller_bytes(self) -> None:
        script = escrow_2of3.unlock_release_seller(
            escrow_2of3.BUYER_HASH,
            escrow_2of3.SELLER_HASH,
            escrow_2of3.ARBITER_HASH,
            2_000_000,
            0,
            escrow_2of3.SELLER_HASH,
            escrow_2of3.ARBITER_HASH,
            1_999_000,
        )
        self.assertTrue(len(script) > 16)

    def test_proof_flow_steps(self) -> None:
        self.assertEqual(escrow_2of3.FLOW, ("genesis", "release_seller"))


if __name__ == "__main__":
    unittest.main()
