"""Kasplex inscription JSON. Does not submit transactions."""

from __future__ import annotations

import sys
import unittest
import asyncio
from pathlib import Path
from unittest.mock import AsyncMock, patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))


class KasplexInscriptionTests(unittest.TestCase):
    def test_mint_envelope_is_compact_lowercase(self) -> None:
        import kasplex_krc20 as krc

        self.assertEqual(
            krc.mint_inscription("TMBMN"),
            '{"p":"krc-20","op":"mint","tick":"tmbmn"}',
        )
        self.assertEqual(krc.FRONTIER_TICK, "TMBMN")
        self.assertEqual(krc.DEPLOY_FEE_SOMPI, 100_000_000_000)
        self.assertEqual(
            krc.deploy_inscription("FRNTR", "2100000000000000", "100000000000"),
            '{"p":"krc-20","op":"deploy","tick":"frntr","max":"2100000000000000","lim":"100000000000","dec":"8"}',
        )
        with self.assertRaises(ValueError):
            krc.mint_inscription("abc")
        with self.assertRaises(ValueError):
            krc.mint_inscription("toolong")
        with self.assertRaises(ValueError):
            krc.mint_inscription("tést")
        with self.assertRaises(ValueError):
            krc.deploy_inscription("frntr", "10", "11")

    def test_transfer_envelope_is_compact_lowercase(self) -> None:
        import kasplex_krc20 as krc

        dest = "kaspatest:qqw455g3c6rwtyk59x7m2vez4xqnnqqrep4stc7tzfn55cheygtesvv9207fe"
        self.assertEqual(
            krc.transfer_inscription("TMBMN", "50000000000", dest),
            '{"p":"krc-20","op":"transfer","tick":"tmbmn","amt":"50000000000","to":"'
            + dest
            + '"}',
        )
        with self.assertRaises(ValueError):
            krc.transfer_inscription("tmbmn", "1", "kaspa:qq")
        with self.assertRaises(ValueError):
            krc.transfer_inscription("tmbmn", "0", dest)
        with self.assertRaises(ValueError):
            krc.transfer_inscription("tmbmn", "-1", dest)
        with self.assertRaises(ValueError):
            krc.transfer_inscription("tmbmn", "1", "kaspatest:not-valid")

    def test_select_commit_balances_storage_mass(self) -> None:
        import kasplex_krc20 as krc

        def entry(amount: int) -> dict:
            return {"utxoEntry": {"amount": amount}}

        selected, commit = krc.select_commit_entries([entry(49_317_500)], 10_000_000)
        self.assertEqual(len(selected), 1)
        self.assertGreaterEqual(commit, krc.MIN_STORAGE_SAFE_SOMPI)
        self.assertGreaterEqual(49_317_500 - commit, krc.MIN_STORAGE_SAFE_SOMPI)

        with self.assertRaises(RuntimeError):
            krc.select_commit_entries([entry(5_000_000)], 10_000_000)

    def test_commit_address_is_testnet_p2sh(self) -> None:
        import kasplex_krc20 as krc
        from kaspa import Keypair, PrivateKey

        key = PrivateKey(str(Keypair.random().private_key))
        xonly = key.to_public_key().to_x_only_public_key().to_string()
        _script, addr = krc.commit_address(xonly, krc.mint_inscription("tmbmn"))
        self.assertTrue(addr.startswith("kaspatest:"))
        self.assertTrue(addr.startswith("kaspatest:p") or "pp" in addr[:20])

    def test_kasplex_get_requires_https(self) -> None:
        import kasplex_krc20 as krc

        old = krc.KASPLEX
        krc.KASPLEX = "http://example.invalid"
        try:
            with self.assertRaises(RuntimeError) as ctx:
                krc.kasplex_get("/info")
            self.assertIn("https", str(ctx.exception))
        finally:
            krc.KASPLEX = old

    def test_mint_response_requires_matching_ticker_and_valid_limit(self) -> None:
        import kasplex_krc20 as krc

        valid = {
            "tick": "TMBMN",
            "max": "100",
            "minted": "20",
            "lim": "10",
            "mod": "mint",
            "state": "deployed",
        }
        self.assertEqual(krc.validate_mint_row("TMBMN", valid), 10)
        for update in [
            {"tick": "OTHER"},
            {"lim": "0"},
            {"lim": "101"},
            {"minted": "95"},
        ]:
            with self.assertRaises(RuntimeError):
                krc.validate_mint_row("TMBMN", {**valid, **update})

    def test_mint_mismatch_never_broadcasts(self) -> None:
        import kasplex_krc20 as krc

        response = {
            "result": [
                {
                    "tick": "OTHER",
                    "max": "100",
                    "minted": "0",
                    "lim": "10",
                    "mod": "mint",
                    "state": "deployed",
                }
            ]
        }
        broadcast = AsyncMock()
        with (
            patch.object(krc, "kasplex_get", return_value=response),
            patch.object(krc, "commit_and_reveal", broadcast),
        ):
            with self.assertRaises(RuntimeError):
                asyncio.run(krc.mint("alice", "TMBMN"))
        broadcast.assert_not_awaited()


if __name__ == "__main__":
    unittest.main()
