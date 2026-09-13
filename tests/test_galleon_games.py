"""Offline tests for Galleon games hub and parity quest scoring."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))

from galleon_games_common import parse_bet  # noqa: E402


class GalleonGamesTests(unittest.TestCase):
    def test_parse_bet(self) -> None:
        self.assertEqual(parse_bet("1.5"), 1_500_000_000_000_000_000)
        self.assertEqual(parse_bet("0.1"), 100_000_000_000_000_000)

    def test_parity_label(self) -> None:
        from parity_quest import parity_label

        self.assertEqual(parity_label(10), "even")
        self.assertEqual(parity_label(11), "odd")

    def test_parity_scores_roundtrip(self) -> None:
        from parity_quest import load_scores, save_scores

        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "scores.json"
            with mock.patch("parity_quest.SCORE_PATH", path):
                save_scores({"best_streak": 3, "total_wins": 5, "total_rounds": 10, "history": []})
                body = load_scores()
            self.assertEqual(body["best_streak"], 3)


if __name__ == "__main__":
    unittest.main()
