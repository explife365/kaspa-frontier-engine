"""Safety tests for the Galleon faucet helper. No live requests or transfers."""

from __future__ import annotations

import sys
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))

import galleon_faucet as faucet  # noqa: E402


class KeyHandlingTests(unittest.TestCase):
    def test_private_key_signing_stays_in_process(self) -> None:
        secret = "0x" + "11" * 32
        address = faucet.address_of(secret)
        signature = faucet.sign_challenge(secret, "test challenge")
        self.assertTrue(address.startswith("0x"))
        self.assertEqual(len(address), 42)
        self.assertTrue(signature.startswith("0x"))
        self.assertEqual(len(signature), 132)

    def test_wrong_chain_refuses_value_transfer(self) -> None:
        with patch.object(faucet, "rpc_hex", return_value="0x1"):
            with self.assertRaises(RuntimeError):
                faucet.require_galleon_chain()

    def test_native_send_is_signed_in_process(self) -> None:
        secret = "0x" + "11" * 32
        submitted: list[str] = []

        def rpc(method: str, params: list[object]) -> str:
            if method == "eth_chainId":
                return hex(faucet.GALLEON_CHAIN_ID)
            if method == "eth_getBalance":
                return hex(10**18)
            if method == "eth_getTransactionCount":
                return "0x0"
            if method == "eth_sendRawTransaction":
                raw = str(params[0])
                self.assertTrue(raw.startswith("0x"))
                self.assertNotIn(secret[2:], raw)
                submitted.append(raw)
                return "0xabc"
            raise AssertionError(method)

        with (
            patch.object(faucet, "rpc_hex", side_effect=rpc),
            patch.object(faucet, "print_balance"),
        ):
            faucet.send_from_key(secret, "0x" + "22" * 20, 1, 2_000_000_000_000)
        self.assertEqual(len(submitted), 1)

    def test_send_rejects_invalid_destination_and_amount_before_rpc(self) -> None:
        with patch.object(faucet, "rpc_hex") as rpc:
            with self.assertRaises(ValueError):
                faucet.send_from_key("0xsecret", "0x1234", 1, 1)
            with self.assertRaises(ValueError):
                faucet.send_from_key("0xsecret", "0x" + "22" * 20, 0, 1)
        rpc.assert_not_called()

    def test_sweep_uses_primary_destination_and_max_safe_amount(self) -> None:
        primary = "0x" + "11" * 20
        extra = "0x" + "22" * 20
        balance = 10**18
        sent = Mock()

        def address(key: str) -> str:
            return primary if key == "primary-key" else extra

        def rpc(method: str, _params: list[object]) -> str:
            if method == "eth_chainId":
                return hex(faucet.GALLEON_CHAIN_ID)
            if method == "eth_getBalance":
                return hex(balance)
            raise AssertionError(method)

        with (
            patch.object(faucet, "galleon_key", return_value="primary-key"),
            patch.object(faucet, "extra_indices", return_value=[2]),
            patch.object(faucet, "load_key", return_value="extra-key"),
            patch.object(faucet, "address_of", side_effect=address),
            patch.object(faucet, "rpc_hex", side_effect=rpc),
            patch.object(faucet, "send_from_key", sent),
            patch.object(faucet, "print_balance"),
        ):
            faucet.sweep_extras_to_primary()

        sent.assert_called_once_with(
            "extra-key",
            primary,
            faucet.max_sendable_wei(
                balance, faucet.NATIVE_TRANSFER_GAS, faucet.GALLEON_MIN_GAS_WEI
            ),
            faucet.GALLEON_MIN_GAS_WEI,
        )


class ClaimDurabilityTests(unittest.TestCase):
    def test_claim_log_is_atomic_and_idempotent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "claims.txt"
            with (
                patch.object(faucet, "CLAIM_LOG", path),
                patch.object(faucet, "utc_claim_day", return_value="2026-08-25"),
            ):
                faucet.remember_claim("0xABC")
                faucet.remember_claim("0xabc")
            self.assertEqual(path.read_text(encoding="utf-8"), "2026-08-25 0xabc\n")

    def test_success_is_recorded_before_optional_balance_refresh(self) -> None:
        remember = Mock()
        with (
            patch.object(faucet, "address_of", return_value="0x" + "22" * 20),
            patch.object(faucet, "claimed_today", return_value=False),
            patch.object(faucet, "print_balance", side_effect=[0.0, RuntimeError("RPC down")]),
            patch.object(
                faucet,
                "sign_faucet_challenge",
                return_value=("challenge", "signature", None),
            ),
            patch.object(faucet, "http_json", return_value={"txHash": "0x123"}),
            patch.object(faucet, "remember_claim", remember),
            patch.object(faucet.time, "sleep"),
        ):
            with self.assertRaisesRegex(RuntimeError, "RPC down"):
                faucet.drip_with_key("0xsecret", "wallet")
        remember.assert_called_once()

    def test_concurrent_process_paths_submit_only_one_claim(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "claims.txt"
            request = Mock(return_value={"txHash": "0x123"})
            with (
                patch.object(faucet, "CLAIM_LOG", path),
                patch.object(faucet, "address_of", return_value="0x" + "22" * 20),
                patch.object(faucet, "print_balance", return_value=0.0),
                patch.object(
                    faucet,
                    "sign_faucet_challenge",
                    return_value=("challenge", "signature", None),
                ),
                patch.object(faucet, "http_json", request),
                patch.object(faucet.time, "sleep"),
            ):
                with ThreadPoolExecutor(max_workers=2) as pool:
                    results = list(pool.map(lambda _: faucet.drip_with_key("key", "wallet"), range(2)))
            self.assertEqual(sorted(results), [False, True])
            request.assert_called_once()


if __name__ == "__main__":
    unittest.main()
