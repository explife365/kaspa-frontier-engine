"""Galleon MiniPool helpers (offline). No live RPC."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))

from galleon_pool import encode_swap, parse_amount, quote_swap  # noqa: E402


class GalleonPoolEncodingTests(unittest.TestCase):
    def test_parse_amount_18_decimals(self) -> None:
        self.assertEqual(parse_amount("1.5", 18), 1_500_000_000_000_000_000)

    def test_encode_swap_selector_and_args(self) -> None:
        data = encode_swap(100, True, 42)
        self.assertTrue(data.startswith("0x4312ae31"))
        self.assertEqual(len(data), 2 + 8 + 64 * 3)


class ConstantProductQuoteTests(unittest.TestCase):
    def test_quote_matches_x_y_k_formula(self) -> None:
        # amountOut = amountIn * rOut / (rIn + amountIn)
        r0, r1 = 1_000 * 10**18, 2_000 * 10**18
        amount_in = 100 * 10**18
        expected = amount_in * r1 // (r0 + amount_in)

        class FakeRpc:
            pass

        def fake_eth_call(_rpc: str, _pool: str, data: str) -> str:
            self.assertTrue(data.startswith("0x3ab1dee3"))
            return "0x" + f"{expected:064x}"

        import galleon_pool as gp

        original = gp.eth_call
        gp.eth_call = fake_eth_call
        try:
            out = quote_swap("http://fake", "0xpool", amount_in, True)
        finally:
            gp.eth_call = original
        self.assertEqual(out, expected)


if __name__ == "__main__":
    unittest.main()
