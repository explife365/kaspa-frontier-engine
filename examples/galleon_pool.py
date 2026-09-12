"""Galleon MiniPool reserves, quotes, and rehearsal swaps.

  python examples/galleon_pool.py --status
  python examples/galleon_pool.py --quote 1.0 --zero-for-one
  python examples/galleon_pool.py --swap 0.1 --zero-for-one --dry-run
  python examples/galleon_pool.py --swap 0.1 --zero-for-one --broadcast

Pool contract is examples/galleon/src/GalleonMiniPool.sol (deploy via forge). Not kaspad.
"""

from __future__ import annotations

import argparse
import sys
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from erc20 import decode_uint256, eth_call, token_balance, token_meta  # noqa: E402
from galleon import GALLEON_CHAIN_ID, GALLEON_GTEST, GALLEON_RPC, GALLEON_WRAPPED_IKAS  # noqa: E402
from galleon_faucet import address_of, galleon_key, require_galleon_chain  # noqa: E402
from galleon_pool_seed import allowance_of, encode_approve, pool_address, send_contract  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402

# keccak256("getReserves()")[:4]
SELECTOR_GET_RESERVES = "0x0902f1ac"
# keccak256("quoteSwap(uint256,bool)")[:4]
SELECTOR_QUOTE_SWAP = "0x3ab1dee3"
# keccak256("swap(uint256,bool,uint256)")[:4]
SELECTOR_SWAP = "0x4312ae31"
APPROVE_GAS = 55_000
SWAP_GAS = 75_000


def _encode_uint256(value: int) -> str:
    return f"{value:064x}"


def _encode_bool(flag: bool) -> str:
    return "0" * 63 + ("1" if flag else "0")


def get_reserves(rpc: str, pool: str) -> tuple[int, int]:
    raw = eth_call(rpc, pool, SELECTOR_GET_RESERVES)
    body = raw.removeprefix("0x")
    if len(body) < 128:
        raise RuntimeError("getReserves returned short data")
    return decode_uint256("0x" + body[:64]), decode_uint256("0x" + body[64:128])


def quote_swap(rpc: str, pool: str, amount_in: int, zero_for_one: bool) -> int:
    data = (
        SELECTOR_QUOTE_SWAP
        + _encode_uint256(amount_in)
        + _encode_bool(zero_for_one)
    )
    return decode_uint256(eth_call(rpc, pool, data))


def parse_amount(text: str, decimals: int) -> int:
    whole, frac = (text.split(".", 1) + ["0"])[:2]
    frac = (frac + "0" * decimals)[:decimals]
    return int(whole) * (10 ** decimals) + int(frac or "0")


def encode_swap(amount_in: int, zero_for_one: bool, min_out: int) -> str:
    return (
        SELECTOR_SWAP
        + _encode_uint256(amount_in)
        + _encode_bool(zero_for_one)
        + _encode_uint256(min_out)
    )


def execute_swap(
    rpc: str,
    pool: str,
    token0: str,
    token1: str,
    amount_in: int,
    zero_for_one: bool,
    min_out: int,
    broadcast: bool,
) -> None:
    require_galleon_chain()
    key = galleon_key()
    owner = address_of(key)
    token_in = token0 if zero_for_one else token1
    token_out = token1 if zero_for_one else token0
    quoted = quote_swap(rpc, pool, amount_in, zero_for_one)
    if min_out > quoted:
        raise RuntimeError(f"min_out {min_out} exceeds quote {quoted}")
    bal_in = token_balance(rpc, token_in, owner)
    if bal_in < amount_in:
        raise RuntimeError(f"insufficient {token_in} balance: have {bal_in}, need {amount_in}")
    steps: list[tuple[str, str, str, int, int]] = []
    if allowance_of(token_in, owner, pool) < amount_in:
        steps.append(
            (
                "approve token in",
                token_in,
                encode_approve(pool, amount_in),
                APPROVE_GAS,
                0,
            )
        )
    steps.append(
        (
            "swap",
            pool,
            encode_swap(amount_in, zero_for_one, min_out),
            SWAP_GAS,
            0,
        )
    )
    print(f"trader  {owner}")
    print(f"in      {amount_in} raw  ({token_in})")
    print(f"out     quoted {quoted} raw min {min_out}  ({token_out})")
    print(f"dir     {'token0->token1' if zero_for_one else 'token1->token0'}")
    for label, to, data, gas, value in steps:
        print(f"{label}  to={to}  gas={gas}")
        if broadcast:
            send_contract(key, to, data, gas, value)


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Galleon MiniPool read-only probe")
    parser.add_argument("--rpc", default=GALLEON_RPC)
    parser.add_argument(
        "--pool",
        default=(os.environ.get("GALLEON_MINI_POOL") or "").strip(),
        help="deployed GalleonMiniPool address",
    )
    parser.add_argument("--token0", default=GALLEON_GTEST)
    parser.add_argument("--token1", default=GALLEON_WRAPPED_IKAS or "")
    parser.add_argument("--quote", type=float, help="human amount of token in")
    parser.add_argument("--swap", type=float, help="human amount in for on-chain swap")
    parser.add_argument("--zero-for-one", action="store_true", help="swap token0 -> token1")
    parser.add_argument("--one-for-zero", action="store_true", help="swap token1 -> token0")
    parser.add_argument(
        "--min-out",
        type=float,
        help="minimum human amount out (default: quoted out)",
    )
    parser.add_argument("--dry-run", action="store_true", help="with --swap: print planned txs")
    parser.add_argument("--broadcast", action="store_true", help="with --swap: sign and send")
    parser.add_argument("--status", action="store_true", help="print reserves (requires --pool)")
    args = parser.parse_args()

    if args.swap is not None and not args.dry_run and not args.broadcast:
        parser.error("--swap requires --dry-run or --broadcast")
    if args.swap is not None and not args.pool:
        try:
            args.pool = pool_address()
        except RuntimeError:
            parser.error("--swap needs --pool or GALLEON_MINI_POOL in kaspa.env")

    print(f"chain  {GALLEON_CHAIN_ID}  rpc {args.rpc}")
    print("not kaspad  not Circle USDC  rehearsal AMM on Galleon L2")

    if args.token0:
        m0 = token_meta(args.rpc, args.token0)
        print(f"token0 {m0['symbol']}  {args.token0}")
    if args.token1:
        m1 = token_meta(args.rpc, args.token1)
        print(f"token1 {m1['symbol']}  {args.token1}")

    if not args.pool:
        print("no --pool address (deploy GalleonMiniPool via forge test / script first)")
        return 0

    r0, r1 = get_reserves(args.rpc, args.pool)
    if args.token0 and args.token1:
        d0 = token_meta(args.rpc, args.token0)["decimals"]
        d1 = token_meta(args.rpc, args.token1)["decimals"]
        print(f"reserves raw  {r0} / {r1}")
        print(f"reserves      {r0 / 10**d0:.6f} {m0['symbol']} / {r1 / 10**d1:.6f} {m1['symbol']}")

    zero_for_one = args.zero_for_one or not args.one_for_zero

    if args.quote is not None:
        if not args.token0:
            raise SystemExit("--quote needs --token0 for decimals")
        token_in = args.token0 if zero_for_one else args.token1
        if not token_in:
            raise SystemExit("--quote needs token addresses for decimals")
        decimals = token_meta(args.rpc, token_in)["decimals"]
        amount_in = parse_amount(str(args.quote), decimals)
        out = quote_swap(args.rpc, args.pool, amount_in, zero_for_one)
        print(f"quote in      {args.quote} (raw {amount_in})")
        print(f"direction     {'token0->token1' if zero_for_one else 'token1->token0'}")
        print(f"amount out    {out} raw")

    if args.swap is not None:
        if not args.token0 or not args.token1:
            raise SystemExit("--swap needs --token0 and --token1")
        token_in = args.token0 if zero_for_one else args.token1
        token_out = args.token1 if zero_for_one else args.token0
        d_in = token_meta(args.rpc, token_in)["decimals"]
        d_out = token_meta(args.rpc, token_out)["decimals"]
        amount_in = parse_amount(str(args.swap), d_in)
        quoted = quote_swap(args.rpc, args.pool, amount_in, zero_for_one)
        if args.min_out is not None:
            min_out = parse_amount(str(args.min_out), d_out)
        else:
            min_out = quoted
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
        if args.broadcast:
            r0, r1 = get_reserves(args.rpc, args.pool)
            print(f"reserves now  {r0} / {r1} raw")

    if args.status or (args.quote is None and args.swap is None):
        print(f"pool          {args.pool}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
