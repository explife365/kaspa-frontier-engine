"""Offline tests for Galleon DEX helpers."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))

from galleon_dex import _zero_for_one  # noqa: E402
from galleon_dex_common import min_out_from_quote, spot_price  # noqa: E402
sys.path.insert(0, str(ROOT / "tests"))
from test_galleon_fee_pool import quote_with_fee  # noqa: E402


class GalleonDexTests(unittest.TestCase):
    def test_zero_for_one(self) -> None:
        self.assertTrue(_zero_for_one("wiKAS"))
        self.assertFalse(_zero_for_one("gTEST"))

    def test_min_out_slippage(self) -> None:
        self.assertEqual(min_out_from_quote(1_000_000, 50), 995_000)
        self.assertEqual(min_out_from_quote(1_000_000, 0), 1_000_000)

    def test_spot_price(self) -> None:
        p = spot_price(1_000 * 10**18, 2_000 * 10**18, 18, 18)
        self.assertAlmostEqual(p, 2.0)

    def test_quote_matches_fee_pool_math(self) -> None:
        amount_in = 10 * 10**18
        r_in = 1_000 * 10**18
        r_out = 500 * 10**18
        out = quote_with_fee(amount_in, r_in, r_out, 30)
        self.assertGreater(out, 0)
        self.assertLess(out, r_out)


if __name__ == "__main__":
    unittest.main()
