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

    def test_require_toccata_sdk_matches_installed_serialize(self) -> None:
        import counter
        from kaspa import Hash, TransactionInput, TransactionOutpoint

        probe = TransactionInput(
            TransactionOutpoint(Hash("00" * 32), 0),
            b"",
            sequence=0,
            sig_op_count=0,
            compute_budget=counter.COMPUTE_BUDGET,
        )
        dropped = probe.to_dict().get("computeBudget") != counter.COMPUTE_BUDGET
        if dropped:
            with self.assertRaisesRegex(RuntimeError, "drops computeBudget"):
                counter.require_toccata_sdk()
            return
        counter.require_toccata_sdk()

    def test_timelock_vault_compiles(self) -> None:
        import timelock_vault

        sample = timelock_vault.compiled_vault(1_000_000)
        self.assertTrue(len(bytes(sample.script)) > 0)
        self.assertIn("TimelockVault", timelock_vault.SOURCE)


    def test_restricted_swap_compiles(self) -> None:
        import restricted_swap

        sample = restricted_swap.compiled_swap(restricted_swap.ALLOWED_RECIPIENT_HASH)
        self.assertTrue(len(bytes(sample.script)) > 0)
        self.assertIn("RestrictedSwap", restricted_swap.SOURCE)


if __name__ == "__main__":
    unittest.main()
