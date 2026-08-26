"""Named TN10 wallets. Never reads the real repo kaspa.env."""

from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from tn10_wallets import (  # noqa: E402
    DEV_DONATION_ADDRESS,
    WALLET_NAMES,
    ensure_wallets,
    get_wallet,
    wallet_key_env,
)


class Tn10WalletTests(unittest.TestCase):
    def setUp(self) -> None:
        self._saved = {k: os.environ.get(k) for k in list(os.environ) if k.startswith("KASPA_")}
        for key in list(os.environ):
            if key.startswith("KASPA_"):
                del os.environ[key]

    def tearDown(self) -> None:
        for key in list(os.environ):
            if key.startswith("KASPA_"):
                del os.environ[key]
        for key, value in self._saved.items():
            if value is not None:
                os.environ[key] = value

    def test_creates_named_testnet_wallets_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            wallets = ensure_wallets(root)
            names = [w.name for w in wallets]
            self.assertEqual(names, list(WALLET_NAMES))
            addresses = {w.address for w in wallets}
            self.assertEqual(len(addresses), len(WALLET_NAMES))
            for wallet in wallets:
                self.assertTrue(wallet.address.startswith("kaspatest:"))
                self.assertIn(wallet.address, wallet.explorer)
            env_text = (root / "kaspa.env").read_text(encoding="utf-8")
            for name in WALLET_NAMES:
                self.assertIn(f"{wallet_key_env(name)}=", env_text)
            again = ensure_wallets(root)
            self.assertEqual([w.address for w in again], [w.address for w in wallets])

    def test_alice_reuses_existing_funding_key(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            from kaspa import Keypair, PrivateKey
            from tn10_wallets import address_from_key

            generated = Keypair.random()
            hex_key = str(generated.private_key)
            expected = address_from_key(PrivateKey(hex_key))
            (root / "kaspa.env").write_text(
                f"KASPA_TN10_FUNDING_KEY={hex_key}\n",
                encoding="utf-8",
            )
            wallets = {w.name: w for w in ensure_wallets(root)}
            self.assertEqual(wallets["alice"].address, expected)
            self.assertNotEqual(wallets["bob"].address, expected)

    def test_unknown_wallet_name(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            ensure_wallets(Path(tmp))
            with self.assertRaises(KeyError):
                get_wallet("mallory", Path(tmp))

    def test_donation_sig_is_mainnet_not_testnet(self) -> None:
        self.assertTrue(DEV_DONATION_ADDRESS.startswith("kaspa:"))
        self.assertFalse(DEV_DONATION_ADDRESS.startswith("kaspatest:"))
        self.assertNotIn("PRIVATE", wallet_key_env("alice"))


if __name__ == "__main__":
    unittest.main()
