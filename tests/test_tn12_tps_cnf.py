"""Rothschild TPS packing. No live TN12 / kaspad. TPS is not BPS."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from tn12_tps_cnf import (  # noqa: E402
    LIVE_BPS,
    MAX_BLOCK_MASS,
    ROTHSCHILD_TPS,
    ROTHSCHILD_TPS_CAP,
    SIMPLE_TX_MASS,
    encode_packing,
    mass_tx_per_block,
    n_slots,
    n_tx,
    packing_fits,
    reduce_rate,
    run_instance,
    solve_cnf,
)


class UnitsTests(unittest.TestCase):
    def test_rothschild_docs_are_tps_not_bps(self) -> None:
        self.assertEqual(ROTHSCHILD_TPS, 5)
        self.assertEqual(ROTHSCHILD_TPS_CAP, 100)
        self.assertEqual(LIVE_BPS, 10)
        self.assertNotEqual(ROTHSCHILD_TPS_CAP, LIVE_BPS)

    def test_100_tps_needs_10_tx_per_block_at_10_bps(self) -> None:
        self.assertTrue(packing_fits(5, 10, 1, 1))
        self.assertFalse(packing_fits(100, 10, 1, 1))
        self.assertFalse(packing_fits(100, 10, 9, 1))
        self.assertTrue(packing_fits(100, 10, 10, 1))
        self.assertEqual(n_tx(100, 1), 100)
        self.assertEqual(n_slots(10, 1), 10)
        self.assertEqual(reduce_rate(100, 10), (10, 1, 10))
        self.assertEqual(reduce_rate(5, 10), (1, 2, 5))

    def test_mass_room_is_far_above_rothschild_cap(self) -> None:
        cap = mass_tx_per_block()
        self.assertEqual(cap, MAX_BLOCK_MASS // SIMPLE_TX_MASS)
        self.assertGreaterEqual(cap, 200)
        self.assertTrue(packing_fits(ROTHSCHILD_TPS_CAP, LIVE_BPS, cap, 1))


class EncodeTests(unittest.TestCase):
    def test_assignment_vars(self) -> None:
        cnf, assign = encode_packing(5, 10, 1, 1)
        self.assertEqual(len([k for k in assign if k.startswith("x_")]), 5 * 10)
        self.assertGreater(len(cnf.clauses), 0)

    def test_empty_tx_is_sat_arithmetic(self) -> None:
        self.assertTrue(packing_fits(0, 10, 1, 1))


class SolverTests(unittest.TestCase):
    def test_tiny_pigeonhole_unsat_if_pysat(self) -> None:
        cnf, _ = encode_packing(3, 2, 1, 1)
        status, _ = solve_cnf(cnf.clauses)
        if status == "no-pysat":
            self.skipTest("python-sat not installed")
        self.assertEqual(status, "UNSAT")
        self.assertFalse(packing_fits(3, 2, 1, 1))

    def test_tiny_fit_sat_if_pysat(self) -> None:
        cnf, _ = encode_packing(2, 2, 1, 1)
        status, model = solve_cnf(cnf.clauses)
        if status == "no-pysat":
            self.skipTest("python-sat not installed")
        self.assertEqual(status, "SAT")
        self.assertIsNotNone(model)

    def test_run_instance_matches_arithmetic(self) -> None:
        import tempfile

        out = Path(tempfile.mkdtemp(prefix="tn12_tps_"))
        row = run_instance(2, 2, 1, 1, out, reduce=False)
        if row["solver"] == "no-pysat":
            self.skipTest("python-sat not installed")
        self.assertTrue(row["match"])
        self.assertEqual(row["arithmetic"], "SAT")
        self.assertIsNotNone(row["slot_counts"])
        self.assertLessEqual(max(row["slot_counts"] or [0]), 1)


if __name__ == "__main__":
    unittest.main()
