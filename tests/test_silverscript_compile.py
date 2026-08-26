"""Compile the TN10 Counter contract without submitting a transaction."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples" / "silverscript"))


class SilverScriptCompileTests(unittest.TestCase):
    def test_counter_compiles(self) -> None:
        import kaspa.experimental.silverscript as silverscript
        import counter

        contract = silverscript.compile(counter.SOURCE, [0])
        self.assertTrue(len(bytes(contract.script)) > 0)
        cached = counter.compiled_counter(0)
        self.assertEqual(bytes(cached.script), bytes(contract.script))

    def test_counter_fails_closed_when_sdk_drops_compute_budget(self) -> None:
        import counter

        with self.assertRaisesRegex(RuntimeError, "drops computeBudget"):
            counter.require_toccata_sdk()


if __name__ == "__main__":
    unittest.main()
