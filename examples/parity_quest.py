"""TN10 Parity Quest — guess even/odd virtual DAA (no gas, pure Kaspa L1 fun).

Inspired by covenant commit-reveal games on TN10. Uses public REST only.

  python examples/parity_quest.py
  python examples/parity_quest.py --rounds 5
  python examples/parity_quest.py --leaderboard
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from tn10_rest import REST_BASE, get_json  # noqa: E402

SCORE_PATH = ROOT / ".local" / "parity_quest_scores.json"
EXPLORER = "https://explorer-tn10.kaspa.org"


def virtual_daa() -> int:
    body = get_json("/info/blockdag")
    return int(body["virtualDaaScore"])


def parity_label(daa: int) -> str:
    return "even" if daa % 2 == 0 else "odd"


def load_scores() -> dict:
    if SCORE_PATH.is_file():
        return json.loads(SCORE_PATH.read_text(encoding="utf-8"))
    return {"best_streak": 0, "total_wins": 0, "total_rounds": 0, "history": []}


def save_scores(data: dict) -> None:
    SCORE_PATH.parent.mkdir(parents=True, exist_ok=True)
    SCORE_PATH.write_text(json.dumps(data, indent=2), encoding="utf-8")


def play_rounds(count: int) -> int:
    scores = load_scores()
    streak = 0
    wins = 0
    print("=== TN10 Parity Quest ===")
    print(f"REST  {REST_BASE}")
    print("Guess the NEXT virtual DAA parity after a short wait.\n")

    for i in range(1, count + 1):
        before = virtual_daa()
        print(f"Round {i}/{count}  current DAA {before} ({parity_label(before)})")
        guess = input("  Your guess [even/odd]: ").strip().lower()
        if guess not in ("even", "odd"):
            print("  skip — need even or odd")
            continue
        print("  waiting for DAA tick...")
        time.sleep(5)
        after = virtual_daa()
        actual = parity_label(after)
        won = guess == actual
        if won:
            wins += 1
            streak += 1
            print(f"  WIN  DAA {after} is {actual}  streak {streak}")
        else:
            streak = 0
            print(f"  miss DAA {after} is {actual}")
        scores["history"].append({"before": before, "after": after, "guess": guess, "won": won})
        if len(scores["history"]) > 50:
            scores["history"] = scores["history"][-50:]

    scores["total_rounds"] = scores.get("total_rounds", 0) + count
    scores["total_wins"] = scores.get("total_wins", 0) + wins
    scores["best_streak"] = max(scores.get("best_streak", 0), streak)
    save_scores(scores)
    print(f"\nScore {wins}/{count} this session  best streak {scores['best_streak']}")
    print("Covenant version: lock a guess on L1 with SilverScript (see DD12 even/odd on TN10)")
    return 0


def show_leaderboard() -> int:
    scores = load_scores()
    print("=== Parity Quest Leaderboard (local) ===")
    print(f"best streak  {scores.get('best_streak', 0)}")
    print(f"total wins   {scores.get('total_wins', 0)} / {scores.get('total_rounds', 0)}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="TN10 parity guessing game")
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--leaderboard", action="store_true")
    args = parser.parse_args()
    if args.leaderboard:
        return show_leaderboard()
    return play_rounds(args.rounds)


if __name__ == "__main__":
    raise SystemExit(main())
