"""Timed gTEST jackpot — buy tickets, draw winner.

  python examples/galleon_jackpot.py --status
  python examples/galleon_jackpot.py --buy-ticket --dry-run
  python examples/galleon_jackpot.py --buy-ticket --broadcast
  python examples/galleon_jackpot.py --draw --broadcast
"""

from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from eth_utils import keccak

from erc20 import decode_uint256, eth_call  # noqa: E402
from galleon import GALLEON_GTEST, GALLEON_RPC  # noqa: E402
from galleon_faucet import galleon_key  # noqa: E402
from galleon_games_common import (  # noqa: E402
    PLAY_GAS,
    banner,
    ensure_approve,
    game_address,
    parse_bet,
    print_wallet_gtest,
    send_contract,
)

SELECTOR_BUY = "0x" + keccak(text="buyTicket()").hex()[:8]
SELECTOR_DRAW = "0x" + keccak(text="draw(uint256)").hex()[:8]
SELECTOR_POT = "0x" + keccak(text="pot()").hex()[:8]
SELECTOR_TICKETS = "0x" + keccak(text="ticketCount()").hex()[:8]
SELECTOR_ENDS = "0x" + keccak(text="roundEndsAt()").hex()[:8]
SELECTOR_ROUND = "0x" + keccak(text="roundId()").hex()[:8]
SELECTOR_PRICE = "0x" + keccak(text="ticketPrice()").hex()[:8]
ROUND_DURATION = 3600


def main() -> int:
    parser = argparse.ArgumentParser(description="Galleon jackpot (gTEST)")
    parser.add_argument("--status", action="store_true")
    parser.add_argument("--buy-ticket", action="store_true")
    parser.add_argument("--draw", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    args = parser.parse_args()

    banner("Kaspa Jackpot")
    pot_addr = game_address("GALLEON_JACKPOT")
    print(f"game    {pot_addr}")
    print_wallet_gtest()

    pot = decode_uint256(eth_call(GALLEON_RPC, pot_addr, SELECTOR_POT))
    tickets = decode_uint256(eth_call(GALLEON_RPC, pot_addr, SELECTOR_TICKETS))
    ends = decode_uint256(eth_call(GALLEON_RPC, pot_addr, SELECTOR_ENDS))
    round_id = decode_uint256(eth_call(GALLEON_RPC, pot_addr, SELECTOR_ROUND))
    price = decode_uint256(eth_call(GALLEON_RPC, pot_addr, SELECTOR_PRICE))
    print(f"round   {round_id}  pot {pot / 1e18:.4f} gTEST  tickets {tickets}")
    print(f"ends    {ends}  ({max(0, int(ends) - int(time.time()))}s left)")

    if args.status and not args.buy_ticket and not args.draw:
        return 0

    if args.buy_ticket:
        if not args.dry_run and not args.broadcast:
            parser.error("pass --dry-run or --broadcast")
        ensure_approve(GALLEON_GTEST, pot_addr, price, args.broadcast)
        if args.broadcast:
            send_contract(galleon_key(), pot_addr, SELECTOR_BUY, PLAY_GAS, 0)
            print("ticket bought")
        else:
            print("dry-run buyTicket OK")
        return 0

    if args.draw:
        if not args.dry_run and not args.broadcast:
            parser.error("pass --dry-run or --broadcast")
        data = SELECTOR_DRAW + f"{ROUND_DURATION:064x}"
        if args.broadcast:
            send_contract(galleon_key(), pot_addr, data, PLAY_GAS, 0)
            print("draw sent")
        else:
            print("dry-run draw OK")
        return 0

    parser.error("pass --status, --buy-ticket, or --draw")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
