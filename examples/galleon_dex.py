"""Galleon DEX hub — quotes, swaps, liquidity on gTEST / wiKAS.

  python examples/galleon_dex.py list
  python examples/galleon_dex.py status
  python examples/galleon_dex.py quote --sell 1.0 --buy wiKAS
  python examples/galleon_dex.py swap --sell 0.5 --buy wiKAS --dry-run
  python examples/galleon_dex.py swap --sell 0.5 --buy wiKAS --broadcast
  python examples/galleon_dex.py add-liquidity --gtest 10 --wikas 0.005 --dry-run
  python examples/galleon_dex.py serve --port 8787

Uses GALLEON_FEE_POOL if set, else GALLEON_MINI_POOL. Not mainnet. gTEST is not USD.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from galleon_dex_common import (  # noqa: E402
    DEFAULT_SLIPPAGE_BPS,
    PAIR_TOKEN0,
    PAIR_TOKEN1,
    dex_quote,
    dex_status,
    pool_address,
    pool_kind,
)
from galleon_faucet import address_of, galleon_key, require_galleon_chain  # noqa: E402
from galleon_pool import execute_swap, parse_amount  # noqa: E402
from galleon_pool_seed import allowance_of, encode_add_liquidity, encode_approve, send_contract  # noqa: E402
from galleon import GALLEON_RPC  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402

HELP = """
Galleon DEX (testnet rehearsal)

  status          reserves, price, wallet balances
  quote           price impact quote (no tx)
  swap            execute swap (--dry-run or --broadcast)
  add-liquidity   seed pool with gTEST + wiKAS
  deploy-fee      deploy GalleonFeePool (30 bps)
  serve           integrator API + DEX routes on loopback

Pair: gTEST / wiKAS on Galleon chain 38836.
"""


def _zero_for_one(buy: str) -> bool:
    buy_l = buy.lower()
    if buy_l in ("wikas", "wiKAS".lower(), "token1", "1"):
        return True
    if buy_l in ("gtest", "token0", "0"):
        return False
    raise ValueError("use --buy gTEST or --buy wiKAS")


def cmd_status(args: argparse.Namespace) -> int:
    body = dex_status(args.rpc)
    if args.json:
        print(json.dumps(body, indent=2))
        return 0
    p = body["pair"]
    r = body["reserves"]
    print(f"pool     {body['pool_kind']}  {body['pool']}")
    print(f"reserves {r['human0']:.4f} {p['token0']['symbol']} / {r['human1']:.6f} {p['token1']['symbol']}")
    print(f"price    {body['price_token1_per_token0']:.8f} wiKAS per gTEST")
    print(f"fee      {body['fee_bps']} bps")
    wb = body["wallet_balances"]
    print(f"wallet   {body['wallet']}")
    print(f"balance  {wb[p['token0']['symbol']]:.4f} gTEST / {wb[p['token1']['symbol']]:.6f} wiKAS")
    if body.get("lp_shares") is not None:
        print(f"lp       {body['lp_shares']} shares (total {body['lp_total_supply']})")
    return 0


def cmd_quote(args: argparse.Namespace) -> int:
    zfo = _zero_for_one(args.buy)
    q = dex_quote(args.sell, zfo, rpc=args.rpc, slippage_bps=args.slippage_bps)
    if args.json:
        print(json.dumps(q, indent=2))
        return 0
    print(f"in   {q['amount_in_human']} {q['token_in']}")
    print(f"out  {q['amount_out_human']:.8f} {q['token_out']}  (min {q['min_out_human']:.8f})")
    print(f"fee  {q['fee_bps']} bps  slippage guard {q['slippage_bps']} bps")
    return 0


def cmd_swap(args: argparse.Namespace) -> int:
    if not args.dry_run and not args.broadcast:
        raise SystemExit("pass --dry-run or --broadcast")
    require_galleon_chain()
    zfo = _zero_for_one(args.buy)
    q = dex_quote(args.sell, zfo, rpc=args.rpc, slippage_bps=args.slippage_bps)
    execute_swap(
        args.rpc,
        pool_address(),
        PAIR_TOKEN0,
        PAIR_TOKEN1,
        q["amount_in"],
        zfo,
        q["min_out"],
        broadcast=args.broadcast,
    )
    return 0


def cmd_add_liquidity(args: argparse.Namespace) -> int:
    if not args.dry_run and not args.broadcast:
        raise SystemExit("pass --dry-run or --broadcast")
    require_galleon_chain()
    load_kaspa_env(ROOT)
    pool = pool_address()
    amount0 = parse_amount(str(args.gtest), 18)
    amount1 = parse_amount(str(args.wikas), 18)
    key = galleon_key()
    owner = address_of(key)
    steps: list[tuple[str, str, str, int, int]] = []
    if allowance_of(PAIR_TOKEN0, owner, pool) < amount0:
        steps.append(("approve gTEST", PAIR_TOKEN0, encode_approve(pool, amount0), 55_000, 0))
    if allowance_of(PAIR_TOKEN1, owner, pool) < amount1:
        steps.append(("approve wiKAS", PAIR_TOKEN1, encode_approve(pool, amount1), 55_000, 0))
    steps.append(("addLiquidity", pool, encode_add_liquidity(amount0, amount1), 120_000, 0))
    print(f"pool  {pool} ({pool_kind()})")
    print(f"add   {args.gtest} gTEST + {args.wikas} wiKAS")
    for label, to, data, gas, value in steps:
        print(f"{label}  gas={gas}")
        if args.broadcast:
            send_contract(key, to, data, gas, value)
    return 0


def cmd_deploy_fee(_: argparse.Namespace) -> int:
    return subprocess.call(
        [sys.executable, str(ROOT / "examples" / "galleon_fee_pool_deploy.py"), "--broadcast"],
        cwd=ROOT,
    )


def cmd_serve(args: argparse.Namespace) -> int:
    return subprocess.call(
        [sys.executable, str(ROOT / "examples" / "integrator_api.py"), "--port", str(args.port)],
        cwd=ROOT,
    )


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Galleon DEX hub", add_help=False)
    parser.add_argument("cmd", nargs="?", default="list")
    parser.add_argument("rest", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    extra = list(args.rest)

    if args.cmd in ("list", "help", "-h", "--help"):
        print(HELP)
        return 0

    sub = argparse.ArgumentParser(description="Galleon DEX")
    sub.add_argument("--rpc", default=GALLEON_RPC)
    sub.add_argument("--json", action="store_true")

    if args.cmd == "status":
        return cmd_status(sub.parse_args(extra))

    if args.cmd == "quote":
        q = sub.add_argument_group("quote")
        q.add_argument("--sell", type=float, required=True)
        q.add_argument("--buy", required=True, help="gTEST or wiKAS")
        q.add_argument("--slippage-bps", type=int, default=DEFAULT_SLIPPAGE_BPS)
        return cmd_quote(sub.parse_args(extra))

    if args.cmd == "swap":
        s = sub.add_argument_group("swap")
        s.add_argument("--sell", type=float, required=True)
        s.add_argument("--buy", required=True)
        s.add_argument("--slippage-bps", type=int, default=DEFAULT_SLIPPAGE_BPS)
        s.add_argument("--dry-run", action="store_true")
        s.add_argument("--broadcast", action="store_true")
        return cmd_swap(sub.parse_args(extra))

    if args.cmd == "add-liquidity":
        a = sub.add_argument_group("liquidity")
        a.add_argument("--gtest", type=float, required=True)
        a.add_argument("--wikas", type=float, required=True)
        a.add_argument("--dry-run", action="store_true")
        a.add_argument("--broadcast", action="store_true")
        return cmd_add_liquidity(sub.parse_args(extra))

    if args.cmd == "deploy-fee":
        return cmd_deploy_fee(sub.parse_args(extra))

    if args.cmd == "serve":
        sub.add_argument("--port", type=int, default=8787)
        return cmd_serve(sub.parse_args(extra))

    print(f"unknown: {args.cmd}\n")
    print(HELP)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
