"""Kaspa DeFi games hub — Galleon L2 + TN10 parity quest.

  python examples/galleon_games.py list
  python examples/galleon_games.py coin-flip --heads --bet 1.0 --dry-run
  python examples/galleon_games.py dice --guess 3 --bet 0.5 --dry-run
  python examples/galleon_games.py jackpot --status
  python examples/galleon_games.py parity --rounds 3
  python examples/galleon_games.py deploy --simulate
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

GAMES = """
Kaspa DeFi Games (testnet rehearsal)

  coin-flip   Heads/tails on Galleon - bet gTEST, win 1.96x
  dice        Guess 1-6, win 4.5x gTEST
  jackpot     Hourly ticket pool on Galleon
  parity      Free TN10 virtual-DAA even/odd quest (no gas)
  deploy      Deploy all three Galleon game contracts
  bench       Run N game txs and collect results (see galleon_games_bench.py)

Examples:
  python examples/galleon_games.py coin-flip --heads --bet 1.0 --dry-run
  python examples/galleon_games.py parity --rounds 5
  python examples/galleon_games.py deploy --simulate

Not mainnet. gTEST is not USD. Coin flip/dice randomness is demo-grade only.
"""


def run_script(script: str, extra: list[str]) -> int:
    cmd = [sys.executable, str(ROOT / "examples" / script)] + extra
    return subprocess.call(cmd)


def main() -> int:
    parser = argparse.ArgumentParser(description="Kaspa DeFi games hub", add_help=False)
    parser.add_argument("game", nargs="?", default="list")
    parser.add_argument("rest", nargs=argparse.REMAINDER)
    args, unknown = parser.parse_known_args()

    if args.game in ("list", "help", "-h", "--help"):
        print(GAMES)
        return 0

    mapping = {
        "coin-flip": "galleon_coin_flip.py",
        "dice": "galleon_dice.py",
        "jackpot": "galleon_jackpot.py",
        "parity": "parity_quest.py",
        "deploy": "galleon_games_deploy.py",
        "bench": "galleon_games_bench.py",
    }
    script = mapping.get(args.game)
    if not script:
        print(f"unknown game: {args.game}\n")
        print(GAMES)
        return 1
    extra = list(args.rest) + unknown
    return run_script(script, extra)


if __name__ == "__main__":
    raise SystemExit(main())
