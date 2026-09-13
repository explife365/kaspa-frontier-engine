"""Shared helpers for Galleon DeFi games."""

from __future__ import annotations

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from erc20 import decode_uint256, eth_call, token_balance, token_meta  # noqa: E402
from galleon import GALLEON_CHAIN_ID, GALLEON_GTEST, GALLEON_RPC  # noqa: E402
from galleon_faucet import address_of, galleon_key, require_galleon_chain  # noqa: E402
from galleon_pool_seed import allowance_of, encode_approve, send_contract  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402

APPROVE_GAS = 55_000
PLAY_GAS = 120_000


def game_address(env_key: str) -> str:
    addr = (os.environ.get(env_key) or "").strip()
    if not addr:
        raise RuntimeError(f"missing {env_key}; run galleon_games_deploy.py --broadcast")
    return addr


def parse_bet(text: str, decimals: int = 18) -> int:
    whole, frac = (text.split(".", 1) + ["0"])[:2]
    frac = (frac + "0" * decimals)[:decimals]
    return int(whole) * (10**decimals) + int(frac or "0")


def pad_uint(value: int) -> str:
    return f"{value:064x}"


def pad_bool(flag: bool) -> str:
    return "0" * 63 + ("1" if flag else "0")


def pad_uint8(value: int) -> str:
    return f"{value:064x}"


def ensure_approve(token: str, spender: str, amount: int, broadcast: bool) -> None:
    owner = address_of(galleon_key())
    if allowance_of(token, owner, spender) >= amount:
        return
    if broadcast:
        send_contract(galleon_key(), token, encode_approve(spender, amount), APPROVE_GAS, 0)
    else:
        print(f"dry-run approve {token} -> {spender}")


def read_stats(rpc: str, pool: str, selectors: dict[str, str]) -> dict[str, int]:
    out: dict[str, int] = {}
    for name, sel in selectors.items():
        out[name] = decode_uint256(eth_call(rpc, pool, sel))
    return out


def print_wallet_gtest(rpc: str = GALLEON_RPC) -> None:
    key = galleon_key()
    owner = address_of(key)
    bal = token_balance(rpc, GALLEON_GTEST, owner)
    meta = token_meta(rpc, GALLEON_GTEST)
    print(f"wallet  {owner}")
    print(f"gTEST   {bal / 10**meta['decimals']:.4f}")


def banner(title: str) -> None:
    require_galleon_chain()
    load_kaspa_env(ROOT)
    print(f"\n=== {title} ===")
    print(f"chain {GALLEON_CHAIN_ID}  Galleon testnet — not mainnet, not USD")
