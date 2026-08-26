"""Unit tests for gitignored kaspa.env load/upsert. Never reads the real repo kaspa.env."""

from __future__ import annotations

import os
import sys
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from kaspa_env import load_kaspa_env, upsert_kaspa_env  # noqa: E402


class KaspaEnvTests(unittest.TestCase):
    def setUp(self) -> None:
        self._saved = {k: os.environ.get(k) for k in list(os.environ) if k.startswith("KASPA_")}
        for key in list(os.environ):
            if key.startswith("KASPA_"):
                del os.environ[key]

    def tearDown(self) -> None:
        for key in list(os.environ):
            if key.startswith("KASPA_"):
                del os.environ[key]
        for key, value in self._saved.items():
            if value is not None:
                os.environ[key] = value

    def test_load_and_upsert_roundtrip(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = upsert_kaspa_env(
                {
                    "KASPA_TN10_FUNDING_KEY": "ab" * 32,
                    "KASPA_FUNDING_KEY": "ab" * 32,
                    "KASPA_RPC_URL": "",
                },
                root,
            )
            self.assertEqual(path.name, "kaspa.env")
            load_kaspa_env(root)
            self.assertEqual(os.environ["KASPA_TN10_FUNDING_KEY"], "ab" * 32)
            self.assertEqual(os.environ["KASPA_FUNDING_KEY"], "ab" * 32)

    def test_process_env_wins(self) -> None:
        os.environ["KASPA_RPC_URL"] = "ws://127.0.0.1:17210"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "kaspa.env").write_text("KASPA_RPC_URL=ws://ignored\n", encoding="utf-8")
            load_kaspa_env(root)
            self.assertEqual(os.environ["KASPA_RPC_URL"], "ws://127.0.0.1:17210")

    def test_upsert_keeps_comments(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "kaspa.env").write_text(
                "# keep me\nKASPA_RPC_URL=ws://old\n# trailing\n",
                encoding="utf-8",
            )
            upsert_kaspa_env({"KASPA_RPC_URL": "ws://new"}, root)
            text = (root / "kaspa.env").read_text(encoding="utf-8")
            self.assertIn("# keep me", text)
            self.assertIn("# trailing", text)
            self.assertIn("KASPA_RPC_URL=ws://new", text)
            self.assertNotIn("ws://old", text)

    def test_concurrent_atomic_upserts_preserve_both_keys(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with ThreadPoolExecutor(max_workers=2) as pool:
                list(
                    pool.map(
                        lambda update: upsert_kaspa_env(update, root),
                        [{"KASPA_KEY_A": "a"}, {"KASPA_KEY_B": "b"}],
                    )
                )
            text = (root / "kaspa.env").read_text(encoding="utf-8")
            self.assertIn("KASPA_KEY_A=a", text)
            self.assertIn("KASPA_KEY_B=b", text)
            self.assertFalse(any(root.glob(".kaspa.env.*")))


if __name__ == "__main__":
    unittest.main()
