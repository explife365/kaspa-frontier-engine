"""Heads or tails on Galleon — bet gTEST, win 1.96x.

  python examples/galleon_coin_flip.py --status
  python examples/galleon_coin_flip.py --heads --bet 1.0 --dry-run
  python examples/galleon_coin_flip.py --tails --bet 0.5 --broadcast
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from eth_utils import keccak

from galleon_games_common import (  # noqa: E402
    PLAY_GAS,
    banner,
    ensure_approve,
    game_address,
    parse_bet,
    print_wallet_gtest,
    send_contract,
)
from galleon_faucet import galleon_key  # noqa: E402
from galleon import GALLEON_GTEST, GALLEON_RPC  # noqa: E402

SELECTOR_FLIP = "0x" + keccak(text="flip(bool,uint256)").hex()[:8]
SELECTOR_GAMES = "0x" + keccak(text="gamesPlayed()").hex()[:8]


def encode_flip(heads: bool, wager: int) -> str:
    return SELECTOR_FLIP + ("0" * 63 + ("1" if heads else "0")) + f"{wager:064x}"


def main() -> int:
    parser = argparse.ArgumentParser(description="Galleon coin flip (gTEST)")
    parser.add_argument("--heads", action="store_true")
    parser.add_argument("--tails", action="store_true")
    parser.add_argument("--bet", default="1.0")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    parser.add_argument("--status", action="store_true")
    args = parser.parse_args()

    banner("Kaspa Coin Flip")
    flip = game_address("GALLEON_COIN_FLIP")
    print(f"game    {flip}")
    print_wallet_gtest()

    if args.status and not args.heads and not args.tails:
        from erc20 import decode_uint256, eth_call

        played = decode_uint256(eth_call(GALLEON_RPC, flip, SELECTOR_GAMES))
        print(f"rounds  {played}")
        return 0

    if not args.heads and not args.tails:
        parser.error("pick --heads or --tails")
    if args.bet and not args.dry_run and not args.broadcast:
        parser.error("pass --dry-run or --broadcast")

    heads = args.heads
    wager = parse_bet(args.bet)
    side = "HEADS" if heads else "TAILS"
    print(f"pick    {side}")
    print(f"wager   {args.bet} gTEST  (payout 1.96x on win)")
    ensure_approve(GALLEON_GTEST, flip, wager, args.broadcast)
    data = encode_flip(heads, wager)
    if args.broadcast:
        send_contract(galleon_key(), flip, data, PLAY_GAS, 0)
        print("flip sent — check explorer for FlipPlayed event")
    else:
        print("dry-run flip OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
