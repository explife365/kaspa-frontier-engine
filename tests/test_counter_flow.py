"""Resume-plan tests. Does not submit transactions."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples" / "silverscript"))


class RemainingFlowTests(unittest.TestCase):
    def test_empty_needs_all_steps(self) -> None:
        import counter

        self.assertEqual(counter.remaining_flow([]), counter.FLOW)

    def test_genesis_only(self) -> None:
        import counter

        self.assertEqual(
            counter.remaining_flow([{"step": "genesis"}]),
            ("add(5)", "subtract(3)"),
        )

    def test_complete(self) -> None:
        import counter

        steps = [{"step": name} for name in counter.FLOW]
        self.assertEqual(counter.remaining_flow(steps), ())

    def test_wrong_order(self) -> None:
        import counter

        with self.assertRaises(ValueError):
            counter.remaining_flow([{"step": "add(5)"}])

    def test_explorer_backfill(self) -> None:
        import counter

        filled, changed = counter.ensure_explorer_urls(
            [{"step": "genesis", "txid": "aa"}]
        )
        self.assertTrue(changed)
        self.assertEqual(
            filled[0]["explorer"],
            "https://explorer-tn10.kaspa.org/txs/aa",
        )
        again, changed_again = counter.ensure_explorer_urls(filled)
        self.assertFalse(changed_again)
        self.assertEqual(again[0]["explorer"], filled[0]["explorer"])


if __name__ == "__main__":
    unittest.main()
