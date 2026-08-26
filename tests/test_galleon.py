"""Galleon / Igra iKAS helpers. No live faucet calls."""

from __future__ import annotations

import hashlib
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from galleon import (  # noqa: E402
    CIRCLE_USDC_ETHEREUM,
    CIRCLE_USDC_ON_GALLEON,
    GALLEON_CHAIN_ID,
    GALLEON_GTEST,
    GALLEON_MIN_GAS_WEI,
    GALLEON_RELAY_GAS_WEI,
    GALLEON_TEST_USDC,
    GALLEON_WRAPPED_IKAS,
    NATIVE_TRANSFER_GAS,
    circle_usdc_on_chain,
    is_circle_usdc,
    claim_recorded_today,
    entry_payload,
    faucet_blocks_address,
    faucet_blocks_connection,
    faucet_drip_body,
    faucet_is_busy,
    fits_balance,
    max_sendable_wei,
    parse_l2_address,
    pow_leading_zero_bits,
    solve_pow,
    stamp_undated_claim_lines,
    tx_total_wei,
    txid_has_galleon_prefix,
)


class TokenAddressTests(unittest.TestCase):
    def test_gtest_is_not_igra_test_usdc(self) -> None:
        self.assertEqual(
            GALLEON_GTEST.lower(),
            "0xbc5e27ab3ce2edb243593cda2437e5b30e0d5d7d",
        )
        self.assertNotEqual(GALLEON_GTEST.lower(), GALLEON_TEST_USDC.lower())
        self.assertEqual(
            GALLEON_WRAPPED_IKAS.lower(),
            "0x7331b0a33ac9aa92f506f057bfaa049ea133f77f",
        )


class CircleUsdcTests(unittest.TestCase):
    def test_galleon_is_not_circle(self) -> None:
        self.assertIsNone(CIRCLE_USDC_ON_GALLEON)
        self.assertIsNone(circle_usdc_on_chain(GALLEON_CHAIN_ID))
        self.assertIsNone(circle_usdc_on_chain(38836))
        self.assertNotEqual(GALLEON_TEST_USDC.lower(), CIRCLE_USDC_ETHEREUM.lower())
        self.assertFalse(is_circle_usdc(GALLEON_CHAIN_ID, GALLEON_TEST_USDC))
        self.assertEqual(
            CIRCLE_USDC_ETHEREUM.lower(),
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        )
        self.assertTrue(is_circle_usdc(1, CIRCLE_USDC_ETHEREUM))
        self.assertFalse(is_circle_usdc(1, GALLEON_TEST_USDC))


class EntryPayloadTests(unittest.TestCase):
    def test_20_kas_little_endian(self) -> None:
        l2 = parse_l2_address("0x00000000000000000000000000000000000000aa")
        payload = entry_payload(l2, 2_000_000_000, 1)
        self.assertEqual(payload[0], 0x92)
        self.assertEqual(payload[20], 0xAA)
        self.assertEqual(payload[21:29], bytes.fromhex("0094357700000000"))
        self.assertEqual(payload[29:33], bytes.fromhex("00000001"))
        self.assertTrue(txid_has_galleon_prefix("97b4abcd"))
        self.assertFalse(txid_has_galleon_prefix("97b1abcd"))


class FaucetPowTests(unittest.TestCase):
    def test_leading_zero_bits(self) -> None:
        self.assertEqual(pow_leading_zero_bits(bytes([0x00, 0x0F])), 12)
        self.assertEqual(pow_leading_zero_bits(bytes([0x80])), 0)
        self.assertEqual(pow_leading_zero_bits(bytes([0x01])), 7)

    def test_solve_easy_pow(self) -> None:
        nonce = solve_pow("test-prefix", 4, limit=50_000)
        digest = hashlib.sha256(f"test-prefix:{nonce}".encode()).digest()
        self.assertGreaterEqual(pow_leading_zero_bits(digest), 4)

    def test_drip_body_omits_nonce_unless_set(self) -> None:
        body = faucet_drip_body("0xabc", "0xsig", "challenge")
        self.assertNotIn("nonce", body)
        self.assertEqual(
            faucet_drip_body("0xabc", "0xsig", "challenge", 9)["nonce"], 9
        )


class IgraGasBudgetTests(unittest.TestCase):
    def test_native_push_fits_our_0_1_ikas(self) -> None:
        balance = 10**17
        value = 10**16
        self.assertTrue(
            fits_balance(balance, NATIVE_TRANSFER_GAS, GALLEON_RELAY_GAS_WEI, value)
        )
        # Igra prepaid gas: 800k * 3000 gwei = 2.4 iKAS. 0.1 iKAS cannot deploy.
        self.assertFalse(fits_balance(balance, 800_000, GALLEON_RELAY_GAS_WEI, 0))
        self.assertFalse(fits_balance(10**16, NATIVE_TRANSFER_GAS, GALLEON_RELAY_GAS_WEI, 10**16))
        self.assertEqual(
            tx_total_wei(NATIVE_TRANSFER_GAS, GALLEON_RELAY_GAS_WEI, 0),
            NATIVE_TRANSFER_GAS * GALLEON_RELAY_GAS_WEI,
        )

    def test_max_sendable_leaves_prepaid_gas(self) -> None:
        balance = 10**17
        sendable = max_sendable_wei(balance)
        self.assertEqual(sendable, balance - NATIVE_TRANSFER_GAS * GALLEON_MIN_GAS_WEI)
        self.assertTrue(
            fits_balance(balance, NATIVE_TRANSFER_GAS, GALLEON_MIN_GAS_WEI, sendable)
        )
        self.assertEqual(max_sendable_wei(NATIVE_TRANSFER_GAS * GALLEON_MIN_GAS_WEI), 0)


class FaucetClaimHelperTests(unittest.TestCase):
    def test_undated_lines_expire_after_stamp_day(self) -> None:
        addr = "0xb39f360afc72908b89aa3413cf0e2eb6d20b4b23"
        self.assertFalse(claim_recorded_today(addr, addr, "2026-08-24"))
        stamped = stamp_undated_claim_lines(addr + "\n", "2026-08-24")
        self.assertTrue(claim_recorded_today(stamped.strip(), addr, "2026-08-24"))
        self.assertFalse(claim_recorded_today(stamped.strip(), addr, "2026-08-25"))

    def test_extras_to_reach_g_test(self) -> None:
        from galleon import extras_to_reach, sweep_net_ikas

        self.assertEqual(extras_to_reach(1.40), 0)
        self.assertAlmostEqual(sweep_net_ikas(), 0.058, places=3)
        # 0.691548 primary needs ~0.71 more; ~13 extras at 0.058 net
        self.assertEqual(extras_to_reach(0.691548), 13)

    def test_extras_to_reach_wikas(self) -> None:
        from galleon import WIKAS_CREATE_IKAS, extras_to_reach

        self.assertEqual(extras_to_reach(WIKAS_CREATE_IKAS, WIKAS_CREATE_IKAS), 0)
        self.assertEqual(extras_to_reach(1.147458, WIKAS_CREATE_IKAS), 7)

    def test_connection_cap_is_not_address_cap(self) -> None:
        conn = "HTTP 429: Daily limit reached for this connection. Try again tomorrow."
        addr = "HTTP 429: this address already claimed today"
        self.assertTrue(faucet_blocks_connection(conn))
        self.assertFalse(faucet_blocks_address(conn))
        self.assertTrue(faucet_blocks_address(addr))
        self.assertFalse(faucet_blocks_connection(addr))

    def test_busy_is_not_a_claim_or_connection_cap(self) -> None:
        busy = "HTTP 429: Faucet is busy. Try again in 94 seconds."
        conn = "HTTP 429: Daily limit reached for this connection. Try again tomorrow."
        addr = "HTTP 429: this address already claimed today"
        self.assertTrue(faucet_is_busy(busy))
        self.assertFalse(faucet_is_busy(conn))
        self.assertFalse(faucet_is_busy(addr))
        self.assertFalse(faucet_blocks_connection(busy))
        self.assertFalse(faucet_blocks_address(busy))


if __name__ == "__main__":
    unittest.main()
