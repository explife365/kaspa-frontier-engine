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


def _sel(sig: str) -> str:
    from eth_utils import keccak

    return "0x" + keccak(text=sig).hex()[:8]


def games_status(rpc: str = GALLEON_RPC) -> dict:
    """Read-only snapshot for integrator API / games UI."""
    import time

    load_kaspa_env(ROOT)
    flip = game_address("GALLEON_COIN_FLIP")
    dice = game_address("GALLEON_DICE")
    jackpot = game_address("GALLEON_JACKPOT")
    gtest_meta = token_meta(rpc, GALLEON_GTEST)
    owner = address_of(galleon_key())
    gtest = token_balance(rpc, GALLEON_GTEST, owner) / 10**gtest_meta["decimals"]
    now = int(time.time())
    ends = decode_uint256(eth_call(rpc, jackpot, _sel("roundEndsAt()")))
    return {
        "chain_id": GALLEON_CHAIN_ID,
        "wallet": owner,
        "gtest_balance": gtest,
        "coin_flip": {
            "address": flip,
            "games_played": decode_uint256(eth_call(rpc, flip, _sel("gamesPlayed()"))),
            "min_bet": 0.1,
            "max_bet": 50.0,
        },
        "dice": {
            "address": dice,
            "rolls": decode_uint256(eth_call(rpc, dice, _sel("rolls()"))),
            "min_bet": 0.1,
            "max_bet": 50.0,
        },
        "jackpot": {
            "address": jackpot,
            "round_id": decode_uint256(eth_call(rpc, jackpot, _sel("roundId()"))),
            "tickets": decode_uint256(eth_call(rpc, jackpot, _sel("ticketCount()"))),
            "pot_gtest": decode_uint256(eth_call(rpc, jackpot, _sel("pot()"))) / 1e18,
            "ticket_price_gtest": decode_uint256(eth_call(rpc, jackpot, _sel("ticketPrice()"))) / 1e18,
            "round_ends_at": ends,
            "seconds_until_draw": max(0, ends - now),
            "draw_ready": now >= ends,
        },
        "cli": {
            "coin_flip": "python examples/galleon_games.py coin-flip --heads --bet 0.1 --broadcast",
            "dice": "python examples/galleon_games.py dice --guess 3 --bet 0.1 --broadcast",
            "jackpot_buy": "python examples/galleon_jackpot.py --buy-ticket --broadcast",
            "jackpot_draw": "python examples/galleon_jackpot.py --draw --broadcast",
        },
        "notes": [
            "Fund each game contract with gTEST bankroll before play (wins revert if empty).",
            "gTEST is not USD. Demo randomness only.",
        ],
    }
