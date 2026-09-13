"""Shared Galleon DEX helpers (gTEST / wiKAS constant-product pools)."""

from __future__ import annotations

import os
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from erc20 import decode_uint256, eth_call, token_balance, token_meta  # noqa: E402
from eth_utils import keccak

from galleon import GALLEON_CHAIN_ID, GALLEON_EXPLORER, GALLEON_GTEST, GALLEON_RPC, GALLEON_WRAPPED_IKAS  # noqa: E402
from galleon_faucet import address_of, galleon_key  # noqa: E402
from galleon_pool import allowance_of, get_reserves, parse_amount, quote_swap  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402

PAIR_TOKEN0 = GALLEON_GTEST
PAIR_TOKEN1 = GALLEON_WRAPPED_IKAS
DEFAULT_SLIPPAGE_BPS = 50


def _selector(sig: str) -> str:
    return "0x" + keccak(text=sig).hex()[:8]


SELECTOR_FEE_BPS = _selector("feeBps()")
SELECTOR_TOTAL_SUPPLY = _selector("totalSupply()")
SELECTOR_BALANCE_OF = _selector("balanceOf(address)")


def pool_address() -> str:
    fee = (os.environ.get("GALLEON_FEE_POOL") or "").strip()
    if fee:
        return fee
    mini = (os.environ.get("GALLEON_MINI_POOL") or "").strip()
    if mini:
        return mini
    raise RuntimeError(
        "missing GALLEON_FEE_POOL or GALLEON_MINI_POOL; deploy pool first"
    )


def pool_kind() -> str:
    fee = (os.environ.get("GALLEON_FEE_POOL") or "").strip()
    return "fee_pool" if fee else "mini_pool"


def read_uint(rpc: str, pool: str, selector: str) -> int:
    return decode_uint256(eth_call(rpc, pool, selector))


def lp_balance(rpc: str, pool: str, owner: str) -> int | None:
    if pool_kind() != "fee_pool":
        return None
    data = SELECTOR_BALANCE_OF + owner.lower().removeprefix("0x").zfill(64)
    return decode_uint256(eth_call(rpc, pool, data))


def fee_bps(rpc: str, pool: str) -> int:
    if pool_kind() != "fee_pool":
        return 0
    return read_uint(rpc, pool, SELECTOR_FEE_BPS)


def min_out_from_quote(quoted: int, slippage_bps: int) -> int:
    bps = max(0, min(slippage_bps, 5_000))
    return quoted * (10_000 - bps) // 10_000


def spot_price(r0: int, r1: int, dec0: int, dec1: int) -> float:
    if r0 == 0:
        return 0.0
    return (r1 / 10**dec1) / (r0 / 10**dec0)


def dex_status(rpc: str = GALLEON_RPC) -> dict[str, Any]:
    load_kaspa_env(ROOT)
    pool = pool_address()
    r0, r1 = get_reserves(rpc, pool)
    m0 = token_meta(rpc, PAIR_TOKEN0)
    m1 = token_meta(rpc, PAIR_TOKEN1)
    kind = pool_kind()
    fb = fee_bps(rpc, pool)
    owner = address_of(galleon_key())
    wallet_gtest = token_balance(rpc, PAIR_TOKEN0, owner)
    wallet_wikas = token_balance(rpc, PAIR_TOKEN1, owner)
    lp = lp_balance(rpc, pool, owner)
    total_lp = read_uint(rpc, pool, SELECTOR_TOTAL_SUPPLY) if kind == "fee_pool" else None
    return {
        "network": "galleon-testnet",
        "chain_id": GALLEON_CHAIN_ID,
        "not_mainnet": True,
        "pool": pool,
        "pool_kind": kind,
        "explorer": GALLEON_EXPLORER,
        "pair": {
            "token0": {"address": m0["address"], "symbol": m0["symbol"], "decimals": m0["decimals"]},
            "token1": {"address": m1["address"], "symbol": m1["symbol"], "decimals": m1["decimals"]},
        },
        "reserves": {
            "raw0": r0,
            "raw1": r1,
            "human0": r0 / 10 ** m0["decimals"],
            "human1": r1 / 10 ** m1["decimals"],
        },
        "price_token1_per_token0": spot_price(r0, r1, m0["decimals"], m1["decimals"]),
        "fee_bps": fb,
        "wallet": owner,
        "wallet_balances": {
            m0["symbol"]: wallet_gtest / 10 ** m0["decimals"],
            m1["symbol"]: wallet_wikas / 10 ** m1["decimals"],
        },
        "lp_shares": lp,
        "lp_total_supply": total_lp,
    }


def dex_quote(
    amount_in_human: float | str,
    zero_for_one: bool,
    *,
    rpc: str = GALLEON_RPC,
    slippage_bps: int = DEFAULT_SLIPPAGE_BPS,
) -> dict[str, Any]:
    load_kaspa_env(ROOT)
    pool = pool_address()
    m0 = token_meta(rpc, PAIR_TOKEN0)
    m1 = token_meta(rpc, PAIR_TOKEN1)
    token_in = PAIR_TOKEN0 if zero_for_one else PAIR_TOKEN1
    token_out = PAIR_TOKEN1 if zero_for_one else PAIR_TOKEN0
    m_in = m0 if zero_for_one else m1
    m_out = m1 if zero_for_one else m0
    amount_in = parse_amount(str(amount_in_human), m_in["decimals"])
    quoted = quote_swap(rpc, pool, amount_in, zero_for_one)
    min_out = min_out_from_quote(quoted, slippage_bps)
    fb = fee_bps(rpc, pool)
    fee_est = amount_in * fb // 10_000 if fb else 0
    return {
        "pool": pool,
        "direction": "token0_to_token1" if zero_for_one else "token1_to_token0",
        "token_in": m_in["symbol"],
        "token_out": m_out["symbol"],
        "amount_in": amount_in,
        "amount_in_human": float(amount_in_human),
        "amount_out": quoted,
        "amount_out_human": quoted / 10 ** m_out["decimals"],
        "min_out": min_out,
        "min_out_human": min_out / 10 ** m_out["decimals"],
        "fee_bps": fb,
        "fee_in_raw": fee_est,
        "slippage_bps": slippage_bps,
    }


def zero_for_one_from_buy_token(buy: str) -> bool:
    b = buy.strip().lower()
    if b in ("gtest", "token0", "0"):
        return False
    if b in ("wikas", "token1", "1"):
        return True
    raise ValueError("buy must be gTEST or wiKAS")


def dex_swap_plan(
    amount_in_human: float | str,
    buy: str,
    *,
    rpc: str = GALLEON_RPC,
    slippage_bps: int = DEFAULT_SLIPPAGE_BPS,
) -> dict[str, Any]:
    """Structured dry-run swap plan (no broadcast)."""
    load_kaspa_env(ROOT)
    zfo = zero_for_one_from_buy_token(buy)
    q = dex_quote(amount_in_human, zfo, rpc=rpc, slippage_bps=slippage_bps)
    owner = address_of(galleon_key())
    token_in = PAIR_TOKEN0 if zfo else PAIR_TOKEN1
    needs_approve = allowance_of(token_in, owner, q["pool"]) < q["amount_in"]
    steps = ["approve", "swap"] if needs_approve else ["swap"]
    return {
        "dry_run": True,
        "not_broadcast": True,
        "trader": owner,
        "steps": steps,
        "needs_approve": needs_approve,
        **q,
    }
