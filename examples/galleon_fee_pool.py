"""Galleon FeePool: fee-aware quotes, treasury status, rehearsal swaps.

  python examples/galleon_fee_pool.py --status
  python examples/galleon_fee_pool.py --quote 1.0 --zero-for-one
  python examples/galleon_fee_pool.py --treasury

Requires GALLEON_FEE_POOL in kaspa.env after galleon_fee_pool_deploy.py --broadcast.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from erc20 import decode_uint256, eth_call, token_balance, token_meta  # noqa: E402
from galleon import GALLEON_CHAIN_ID, GALLEON_GTEST, GALLEON_RPC, GALLEON_WRAPPED_IKAS  # noqa: E402
from galleon_faucet import address_of, galleon_key, require_galleon_chain  # noqa: E402
from galleon_pool import (  # noqa: E402
    encode_swap,
    execute_swap,
    get_reserves,
    parse_amount,
    quote_swap,
)
from eth_utils import keccak

from galleon_pool_seed import send_contract  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402


def _selector(sig: str) -> str:
    return "0x" + keccak(text=sig).hex()[:8]


SELECTOR_FEE_BPS = _selector("feeBps()")
SELECTOR_TREASURY0 = _selector("treasury0()")
SELECTOR_TREASURY1 = _selector("treasury1()")
SELECTOR_WITHDRAW_TREASURY = _selector("withdrawTreasury(address)")


def fee_pool_address() -> str:
    addr = (os.environ.get("GALLEON_FEE_POOL") or "").strip()
    if not addr:
        raise RuntimeError("missing GALLEON_FEE_POOL; run galleon_fee_pool_deploy.py --broadcast")
    return addr


def read_uint(rpc: str, pool: str, selector: str) -> int:
    return decode_uint256(eth_call(rpc, pool, selector))


def treasury_status(rpc: str, pool: str) -> dict:
    return {
        "treasury0": read_uint(rpc, pool, SELECTOR_TREASURY0),
        "treasury1": read_uint(rpc, pool, SELECTOR_TREASURY1),
        "fee_bps": read_uint(rpc, pool, SELECTOR_FEE_BPS),
    }


def encode_withdraw_treasury(to: str) -> str:
    return SELECTOR_WITHDRAW_TREASURY + to.lower().removeprefix("0x").zfill(64)


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Galleon FeePool (30 bps rehearsal)")
    parser.add_argument("--rpc", default=GALLEON_RPC)
    parser.add_argument("--pool", default=(os.environ.get("GALLEON_FEE_POOL") or "").strip())
    parser.add_argument("--token0", default=GALLEON_GTEST)
    parser.add_argument("--token1", default=GALLEON_WRAPPED_IKAS or "")
    parser.add_argument("--quote", type=float)
    parser.add_argument("--swap", type=float)
    parser.add_argument("--zero-for-one", action="store_true")
    parser.add_argument("--one-for-zero", action="store_true")
    parser.add_argument("--min-out", type=float)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    parser.add_argument("--treasury", action="store_true")
    parser.add_argument("--withdraw-treasury", action="store_true")
    parser.add_argument("--status", action="store_true")
    args = parser.parse_args()

    if not args.pool:
        try:
            args.pool = fee_pool_address()
        except RuntimeError:
            print("no GALLEON_FEE_POOL; deploy with galleon_fee_pool_deploy.py")
            return 0

    require_galleon_chain()
    print(f"chain  {GALLEON_CHAIN_ID}  rpc {args.rpc}")
    print("fee pool rehearsal — swap fees accrue to treasury + LPs")

    r0, r1 = get_reserves(args.rpc, args.pool)
    m0 = token_meta(args.rpc, args.token0)
    m1 = token_meta(args.rpc, args.token1)
    print(f"reserves {r0 / 10**m0['decimals']:.6f} {m0['symbol']} / {r1 / 10**m1['decimals']:.6f} {m1['symbol']}")

    tre = treasury_status(args.rpc, args.pool)
    print(f"fee      {tre['fee_bps']} bps")
    print(
        f"treasury pending {tre['treasury0']} raw {m0['symbol']} / "
        f"{tre['treasury1']} raw {m1['symbol']}"
    )

    if args.withdraw_treasury:
        key = galleon_key()
        owner = address_of(key)
        data = encode_withdraw_treasury(owner)
        if args.broadcast:
            send_contract(key, args.pool, data, 80_000, 0)
            print(f"treasury withdrawn to {owner}")
        else:
            print(f"dry-run withdrawTreasury({owner})")

    zero_for_one = args.zero_for_one or not args.one_for_zero
    if args.quote is not None:
        token_in = args.token0 if zero_for_one else args.token1
        decimals = token_meta(args.rpc, token_in)["decimals"]
        amount_in = parse_amount(str(args.quote), decimals)
        out = quote_swap(args.rpc, args.pool, amount_in, zero_for_one)
        fee_raw = amount_in * tre["fee_bps"] // 10_000
        print(f"quote in  {args.quote}  fee ~{fee_raw / 10**decimals:.6f}  out raw {out}")

    if args.swap is not None:
        token_in = args.token0 if zero_for_one else args.token1
        token_out = args.token1 if zero_for_one else args.token0
        d_in = token_meta(args.rpc, token_in)["decimals"]
        d_out = token_meta(args.rpc, token_out)["decimals"]
        amount_in = parse_amount(str(args.swap), d_in)
        quoted = quote_swap(args.rpc, args.pool, amount_in, zero_for_one)
        min_out = parse_amount(str(args.min_out), d_out) if args.min_out is not None else quoted
        execute_swap(
            args.rpc,
            args.pool,
            args.token0,
            args.token1,
            amount_in,
            zero_for_one,
            min_out,
            broadcast=args.broadcast,
        )

    if args.treasury or args.status:
        print(f"pool {args.pool}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
