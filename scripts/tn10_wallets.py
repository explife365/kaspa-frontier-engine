"""Named TN10 test wallets (alice, bob, carol, dave, eve). Keys stay in kaspa.env.

Independent dev sig / optional mainnet KAS (not the Kaspa Dev Fund):
    kaspa:qpxdemlyx445kt5xteux0qhadaw8lh5m0vnqvcy8fh483t70usgkkeulsx9cm
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path

from kaspa import PrivateKey

from kaspa_env import load_kaspa_env, upsert_kaspa_env

NETWORK_TYPE = "testnet"
WALLET_NAMES = ("alice", "bob", "carol", "dave", "eve")
EXPLORER = "https://explorer-tn10.kaspa.org"
DEV_DONATION_ADDRESS = (
    "kaspa:qpxdemlyx445kt5xteux0qhadaw8lh5m0vnqvcy8fh483t70usgkkeulsx9cm"
)


def wallet_key_env(name: str) -> str:
    return f"KASPA_TN10_WALLET_{name.upper()}_KEY"


@dataclass(frozen=True)
class Tn10Wallet:
    name: str
    address: str

    @property
    def explorer(self) -> str:
        return f"{EXPLORER}/addresses/{self.address}"


def _private_key_from_env(name: str) -> PrivateKey | None:
    raw = (os.environ.get(wallet_key_env(name)) or "").strip()
    if name == "alice" and not raw:
        raw = (
            os.environ.get("KASPA_TN10_FUNDING_KEY") or os.environ.get("KASPA_FUNDING_KEY") or ""
        ).strip()
    if not raw:
        return None
    return PrivateKey(raw)


def address_from_key(key: PrivateKey) -> str:
    text = str(key.to_public_key().to_address(NETWORK_TYPE))
    if not text.startswith("kaspatest:"):
        raise RuntimeError(f"TN10 wallet refused non-testnet address: {text}")
    return text


def key_for(name: str) -> PrivateKey:
    key = _private_key_from_env(name)
    if key is None:
        raise KeyError(f"missing TN10 wallet {name}; run ensure_wallets() first")
    return key


def wallet_from_key(name: str, key: PrivateKey) -> Tn10Wallet:
    return Tn10Wallet(name=name, address=address_from_key(key))


def ensure_wallets(root: Path | None = None) -> list[Tn10Wallet]:
    """Create any missing named TN10 keys in kaspa.env. Never prints keys."""
    from kaspa import Keypair

    load_kaspa_env(root)
    updates: dict[str, str] = {}
    wallets: list[Tn10Wallet] = []

    for name in WALLET_NAMES:
        key = _private_key_from_env(name)
        if key is None:
            generated = Keypair.random()
            hex_key = str(generated.private_key)
            key = PrivateKey(hex_key)
            updates[wallet_key_env(name)] = hex_key
            if name == "alice":
                updates.setdefault("KASPA_TN10_FUNDING_KEY", hex_key)
                updates.setdefault("KASPA_FUNDING_KEY", hex_key)
        elif name == "alice":
            hex_key = key.to_string()
            if not (os.environ.get("KASPA_TN10_FUNDING_KEY") or "").strip():
                updates["KASPA_TN10_FUNDING_KEY"] = hex_key
            if not (os.environ.get("KASPA_FUNDING_KEY") or "").strip():
                updates["KASPA_FUNDING_KEY"] = hex_key
        wallets.append(wallet_from_key(name, key))

    if updates:
        upsert_kaspa_env(updates, root)
        os.environ.update(updates)

    return wallets


def get_wallet(name: str, root: Path | None = None) -> Tn10Wallet:
    ensure_wallets(root)
    wanted = name.strip().lower()
    if wanted not in WALLET_NAMES:
        raise KeyError(f"unknown wallet {name!r}; expected {WALLET_NAMES}")
    return wallet_from_key(wanted, key_for(wanted))
