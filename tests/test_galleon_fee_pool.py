"""Offline fee math for GalleonFeePool quotes."""

from __future__ import annotations

import unittest


def quote_with_fee(amount_in: int, reserve_in: int, reserve_out: int, fee_bps: int) -> int:
    amount_in_after_fee = amount_in * (10_000 - fee_bps) // 10_000
    return amount_in_after_fee * reserve_out // (reserve_in + amount_in_after_fee)


class GalleonFeePoolMathTests(unittest.TestCase):
    def test_fee_reduces_output_vs_zero_fee(self) -> None:
        amount_in = 100 * 10**18
        r_in = 1_000 * 10**18
        r_out = 2_000 * 10**18
        zero_fee = quote_with_fee(amount_in, r_in, r_out, 0)
        with_fee = quote_with_fee(amount_in, r_in, r_out, 30)
        self.assertLess(with_fee, zero_fee)

    def test_protocol_fee_split(self) -> None:
        amount_in = 100 * 10**18
        fee_bps = 30
        protocol_share_bps = 5_000
        fee_in = amount_in * fee_bps // 10_000
        protocol = fee_in * protocol_share_bps // 10_000
        lp = fee_in - protocol
        self.assertEqual(protocol + lp, fee_in)


if __name__ == "__main__":
    unittest.main()
