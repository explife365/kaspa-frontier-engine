"""REST helpers. Does not hit the live explorer."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from tn10_rest import (  # noqa: E402
    COINBASE_MATURITY_DAA,
    MAX_STORAGE_MASS,
    confirm_withdrawal,
    decode_json_body,
    encode_path_segment,
    https_request,
    is_mature_utxo,
    retry_after_wait,
    select_commit_entries,
    spendable_entries,
    storage_mass,
)
import tn10_rest  # noqa: E402


def _entry(coinbase: bool, block_daa: int) -> dict:
    return {
        "utxoEntry": {
            "amount": 100_000_000,
            "isCoinbase": coinbase,
            "blockDaaScore": block_daa,
        }
    }


class MatureUtxoTests(unittest.TestCase):
    def test_non_coinbase_is_spendable(self) -> None:
        self.assertTrue(is_mature_utxo(_entry(False, 1), 1))
        self.assertTrue(is_mature_utxo(_entry(False, 1), None))

    def test_coinbase_needs_1000_daa(self) -> None:
        self.assertFalse(is_mature_utxo(_entry(True, 100), 1099))
        self.assertTrue(is_mature_utxo(_entry(True, 100), 1100))
        self.assertFalse(is_mature_utxo(_entry(True, 100), None))
        self.assertEqual(COINBASE_MATURITY_DAA, 1000)

    def test_spendable_filter(self) -> None:
        entries = [_entry(True, 1), _entry(False, 1)]
        self.assertEqual(len(spendable_entries(entries, 500)), 1)
        self.assertEqual(len(spendable_entries(entries, 2000)), 2)


class PathAndStorageMassTests(unittest.TestCase):
    def test_encodes_kaspatest_colon(self) -> None:
        self.assertEqual(encode_path_segment("kaspatest:abc"), "kaspatest%3Aabc")

    def test_decode_json_plain_and_gzip(self) -> None:
        import gzip

        self.assertEqual(decode_json_body(b'{"ok":true}'), {"ok": True})
        packed = gzip.compress(b'{"n":1}')
        self.assertEqual(decode_json_body(packed, "gzip"), {"n": 1})
        self.assertEqual(decode_json_body(packed), {"n": 1})

    def test_retry_after_is_capped_at_two_seconds(self) -> None:
        self.assertEqual(retry_after_wait(None, 0), 0.15)
        self.assertEqual(retry_after_wait("1", 0), 1.0)
        self.assertEqual(retry_after_wait("30", 0), 2.0)
        self.assertEqual(retry_after_wait("nope", 1), 0.3)

    def test_https_request_rejects_cleartext(self) -> None:
        with self.assertRaises(RuntimeError) as ctx:
            https_request("POST", "http://127.0.0.1:9/rpc", {}, 1.0, b"{}")
        self.assertIn("https", str(ctx.exception))

    def test_address_helpers_do_not_turn_backend_failures_into_zero(self) -> None:
        with patch.object(tn10_rest, "get_json", side_effect=RuntimeError("REST 503")):
            with self.assertRaisesRegex(RuntimeError, "503"):
                tn10_rest.address_balance_sompi("kaspatest:abc")
            with self.assertRaisesRegex(RuntimeError, "503"):
                tn10_rest.address_utxos("kaspatest:abc")

    def test_address_helpers_reject_malformed_success_payloads(self) -> None:
        with patch.object(tn10_rest, "get_json", return_value={"balance": "bad"}):
            with self.assertRaisesRegex(RuntimeError, "balance"):
                tn10_rest.address_balance_sompi("kaspatest:abc")
        with patch.object(tn10_rest, "get_json", return_value={"entries": []}):
            with self.assertRaisesRegex(RuntimeError, "UTXO"):
                tn10_rest.address_utxos("kaspatest:abc")

    def test_tiny_output_exceeds_storage_mass(self) -> None:
        mass = storage_mass([49_317_500], [10_000_000, 39_317_500])
        self.assertGreater(mass, MAX_STORAGE_MASS)

    def test_balanced_outputs_are_storage_safe(self) -> None:
        mass = storage_mass([49_317_500], [24_658_750, 24_658_750])
        self.assertLessEqual(mass, MAX_STORAGE_MASS)
        selected, commit = select_commit_entries(
            [{"utxoEntry": {"amount": 49_317_500}}], 10_000_000
        )
        self.assertEqual(len(selected), 1)
        self.assertGreaterEqual(commit, 20_000_000)
        self.assertGreaterEqual(49_317_500 - commit, 21_000_000)

    def test_two_barely_safe_inputs_leave_fee_pad_on_change(self) -> None:
        from tn10_rest import FEE_PAD_SOMPI, MIN_STORAGE_SAFE_SOMPI

        a = {"utxoEntry": {"amount": 22_000_000}}
        b = {"utxoEntry": {"amount": 22_000_000}}
        selected, commit = select_commit_entries([a, b], MIN_STORAGE_SAFE_SOMPI)
        total = 44_000_000
        change = total - commit
        self.assertEqual(len(selected), 2)
        self.assertGreaterEqual(commit, MIN_STORAGE_SAFE_SOMPI)
        self.assertGreaterEqual(change, MIN_STORAGE_SAFE_SOMPI + FEE_PAD_SOMPI)


class WithdrawalConfirmTests(unittest.TestCase):
    def test_waits_for_daa_not_just_appearance(self) -> None:
        utxos = [
            {
                "address": "kaspatest:bob",
                "outpoint": {"transactionId": "w1", "index": 0},
                "utxoEntry": {"amount": 25_000_000, "blockDaaScore": 1000},
            }
        ]
        args = ("w1", "kaspatest:bob", 0, 25_000_000, utxos)
        self.assertIsNone(confirm_withdrawal(*args, 1000, 60))
        self.assertIsNone(confirm_withdrawal(*args, 1059, 60))
        hit = confirm_withdrawal(*args, 1060, 60)
        assert hit is not None
        self.assertEqual(hit["confirmations"], 60)
        self.assertEqual(hit["amount_sompi"], 25_000_000)
        self.assertIsNone(
            confirm_withdrawal(
                "other", "kaspatest:bob", 0, 25_000_000, utxos, 2000, 1
            )
        )
        self.assertIsNone(
            confirm_withdrawal(
                "w1", "kaspatest:bob", 1, 25_000_000, utxos, 2000, 1
            )
        )
        self.assertIsNone(
            confirm_withdrawal(
                "w1", "kaspatest:bob", 0, 1, utxos, 2000, 1
            )
        )


if __name__ == "__main__":
    unittest.main()
