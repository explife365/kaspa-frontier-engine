"""Seed GalleonMiniPool with gTEST / wiKAS rehearsal liquidity.

  python examples/galleon_pool_seed.py --dry-run
  python examples/galleon_pool_seed.py --broadcast

Requires GALLEON_MINI_POOL and GALLEON_PRIVATE_KEY in kaspa.env.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from eth_account import Account
from eth_utils import to_checksum_address

from erc20 import eth_call, decode_uint256, token_balance  # noqa: E402
from galleon import (  # noqa: E402
    GALLEON_CHAIN_ID,
    GALLEON_EXPLORER,
    GALLEON_GTEST,
    GALLEON_MIN_GAS_WEI,
    GALLEON_RPC,
    GALLEON_WRAPPED_IKAS,
    fits_balance,
)
from galleon_faucet import address_of, galleon_key, require_galleon_chain, rpc_hex  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402

SELECTOR_APPROVE = "095ea7b3"
SELECTOR_DEPOSIT = "d0e30db0"
SELECTOR_ADD_LIQ = "9cd441da"
WRAP_WEI = int(0.005 * 1e18)
AMOUNT0 = int(10 * 1e18)
AMOUNT1 = int(0.005 * 1e18)
DEPOSIT_GAS = 50_000
APPROVE_GAS = 55_000
ADD_LIQ_GAS = 100_000
SELECTOR_ALLOWANCE = "dd62ed3e"


def pool_address() -> str:
    addr = (os.environ.get("GALLEON_MINI_POOL") or "").strip()
    if not addr:
        raise RuntimeError("missing GALLEON_MINI_POOL; run galleon_pool_deploy.py --broadcast first")
    return addr


def _pad_addr(addr: str) -> str:
    return addr.lower().removeprefix("0x").zfill(64)


def _pad_uint(value: int) -> str:
    return f"{value:064x}"


def encode_approve(spender: str, amount: int) -> str:
    return "0x" + SELECTOR_APPROVE + _pad_addr(spender) + _pad_uint(amount)


def encode_deposit() -> str:
    return "0x" + SELECTOR_DEPOSIT


def encode_add_liquidity(amount0: int, amount1: int) -> str:
    return "0x" + SELECTOR_ADD_LIQ + _pad_uint(amount0) + _pad_uint(amount1)


def allowance_of(token: str, owner: str, spender: str) -> int:
    data = "0x" + SELECTOR_ALLOWANCE + _pad_addr(owner) + _pad_addr(spender)
    return decode_uint256(eth_call(GALLEON_RPC, token, data))


def send_contract(
    key: str,
    to: str,
    data: str,
    gas: int,
    value: int = 0,
) -> str:
    src = address_of(key)
    bal = int(rpc_hex("eth_getBalance", [src, "latest"]), 16)
    if not fits_balance(bal, gas, GALLEON_MIN_GAS_WEI, value):
        need = gas * GALLEON_MIN_GAS_WEI + value
        raise RuntimeError(f"would be dropped on Igra: have {bal} wei, need {need}")
    nonce = int(rpc_hex("eth_getTransactionCount", [src, "pending"]), 16)
    tx = {
        "chainId": GALLEON_CHAIN_ID,
        "nonce": nonce,
        "to": to_checksum_address(to),
        "value": value,
        "gas": gas,
        "gasPrice": GALLEON_MIN_GAS_WEI,
    }
    tx["data"] = data
    signed = Account.sign_transaction(tx, key)
    raw = signed.raw_transaction.hex()
    if not raw.startswith("0x"):
        raw = "0x" + raw
    tx_hash = rpc_hex("eth_sendRawTransaction", [raw])
    print(f"tx    {tx_hash}")
    print(f"      {GALLEON_EXPLORER}/tx/{tx_hash}")
    return tx_hash


def seed_liquidity(broadcast: bool) -> None:
    require_galleon_chain()
    key = galleon_key()
    pool = pool_address()
    owner = address_of(key)
    wikas_bal = token_balance(GALLEON_RPC, GALLEON_WRAPPED_IKAS, owner)
    steps: list[tuple[str, str, str, int, int]] = []
    if wikas_bal < AMOUNT1:
        steps.append(("wrap wiKAS", GALLEON_WRAPPED_IKAS, encode_deposit(), DEPOSIT_GAS, WRAP_WEI))
    if allowance_of(GALLEON_GTEST, owner, pool) < AMOUNT0:
        steps.append(("approve gTEST", GALLEON_GTEST, encode_approve(pool, AMOUNT0), APPROVE_GAS, 0))
    if allowance_of(GALLEON_WRAPPED_IKAS, owner, pool) < AMOUNT1:
        steps.append(("approve wiKAS", GALLEON_WRAPPED_IKAS, encode_approve(pool, AMOUNT1), APPROVE_GAS, 0))
    steps.append(("addLiquidity", pool, encode_add_liquidity(AMOUNT0, AMOUNT1), ADD_LIQ_GAS, 0))
    for label, to, data, gas, value in steps:
        print(f"{label}  to={to}  gas={gas}  value={value / 1e18:.6f} iKAS")
        if broadcast:
            send_contract(key, to, data, gas, value)


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Seed GalleonMiniPool liquidity on Galleon testnet")
    parser.add_argument("--dry-run", action="store_true", help="print planned txs only")
    parser.add_argument("--broadcast", action="store_true")
    args = parser.parse_args()
    if not args.dry_run and not args.broadcast:
        parser.error("pass --dry-run or --broadcast")
    print(f"chain  {GALLEON_CHAIN_ID}  pool {pool_address()}")
    print(f"pair   {AMOUNT0 / 1e18:.0f} gTEST / {AMOUNT1 / 1e18:.3f} wiKAS")
    seed_liquidity(broadcast=args.broadcast)
    if args.broadcast:
        print("seed OK — run: python examples/galleon_pool.py --status")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
