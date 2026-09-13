"""Tests for developer quickstart router."""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))

from dev_quickstart import PATHS, build_card, detect_path  # noqa: E402


class DevQuickstartTests(unittest.TestCase):
    def test_four_paths_defined(self) -> None:
        self.assertEqual(set(PATHS.keys()), {"rest", "galleon", "nodes", "covenant"})

    def test_rest_path_has_media(self) -> None:
        card = build_card("rest")
        self.assertIn("video", card["media"])
        self.assertTrue(any("tn10-proof" in s for s in card["steps"]))

    def test_detect_defaults_rest_without_env(self) -> None:
        with mock.patch.dict("os.environ", {}, clear=True), mock.patch("dev_quickstart.load_kaspa_env"):
            self.assertEqual(detect_path(), "rest")

    def test_json_serializable(self) -> None:
        json.dumps(build_card("galleon"))


if __name__ == "__main__":
    unittest.main()
