"""ERC-20 ABI decode helpers. Does not hit Galleon."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from erc20 import (  # noqa: E402
    balance_of_calldata,
    decode_abi_string,
    decode_uint256,
    parse_rpc_batch_hex,
    rpc_call,
)


class Erc20AbiTests(unittest.TestCase):
    def test_balance_of_calldata(self) -> None:
        data = balance_of_calldata("0x00000000000000000000000000000000000000aa")
        self.assertTrue(data.startswith("0x70a08231"))
        self.assertTrue(data.endswith("00000000000000000000000000000000000000aa"))
        self.assertEqual(len(data), 2 + 8 + 64)

    def test_decode_usdc_string(self) -> None:
        hex_result = (
            "0x"
            "0000000000000000000000000000000000000000000000000000000000000020"
            "0000000000000000000000000000000000000000000000000000000000000004"
            "5553444300000000000000000000000000000000000000000000000000000000"
        )
        self.assertEqual(decode_abi_string(hex_result), "USDC")
        self.assertEqual(decode_uint256("0x06"), 6)

    def test_rpc_call_requires_https(self) -> None:
        with self.assertRaises(RuntimeError) as ctx:
            rpc_call("http://127.0.0.1:9", "eth_chainId", [])
        self.assertIn("https", str(ctx.exception))

    def test_parse_rpc_batch_hex_orders_by_id(self) -> None:
        parsed = [
            {"id": 3, "result": "0x06"},
            {"id": 1, "result": "0xaa"},
            {"id": 2, "result": "0xbb"},
        ]
        self.assertEqual(parse_rpc_batch_hex(parsed, 3), ["0xaa", "0xbb", "0x06"])
        self.assertIsNone(parse_rpc_batch_hex([{"id": 1, "result": "0xaa"}], 3))
        self.assertIsNone(parse_rpc_batch_hex({"result": "0xaa"}, 1))
        self.assertIsNone(parse_rpc_batch_hex([{"id": 1, "error": {"code": -32000}}], 1))


if __name__ == "__main__":
    unittest.main()
