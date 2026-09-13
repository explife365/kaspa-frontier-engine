"""Roll the Kaspa dice — guess 1-6, win 4.5x gTEST.

  python examples/galleon_dice.py --guess 4 --bet 1.0 --dry-run
  python examples/galleon_dice.py --guess 6 --bet 0.2 --broadcast
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
from galleon import GALLEON_GTEST  # noqa: E402

SELECTOR_ROLL = "0x" + keccak(text="rollDice(uint8,uint256)").hex()[:8]


def encode_roll(guess: int, wager: int) -> str:
    return SELECTOR_ROLL + f"{guess:064x}" + f"{wager:064x}"


def main() -> int:
    parser = argparse.ArgumentParser(description="Galleon dice (gTEST)")
    parser.add_argument("--guess", type=int, required=True, choices=range(1, 7))
    parser.add_argument("--bet", default="1.0")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    args = parser.parse_args()
    if not args.dry_run and not args.broadcast:
        parser.error("pass --dry-run or --broadcast")

    banner("Kaspa Dice")
    dice = game_address("GALLEON_DICE")
    print(f"game    {dice}")
    print_wallet_gtest()
    wager = parse_bet(args.bet)
    print(f"guess   {args.guess}")
    print(f"wager   {args.bet} gTEST  (payout 4.5x on exact match)")
    ensure_approve(GALLEON_GTEST, dice, wager, args.broadcast)
    if args.broadcast:
        send_contract(galleon_key(), dice, encode_roll(args.guess, wager), PLAY_GAS, 0)
        print("roll sent — check DiceRolled event")
    else:
        print("dry-run roll OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
