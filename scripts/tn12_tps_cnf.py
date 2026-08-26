#!/usr/bin/env python3
"""TN12 Rothschild TPS packing — toy CNF.

TPS = transactions per second. Not BPS (blocks per second).

Official rusty-kaspa docs/testnet12.md:
  Rothschild `-t=5` broadcasts 5 transactions per second.
  Participants are asked not to go above 100 TPS.
  TN12 is the covenants experiment (`--netsuffix=12`), not 100 BPS.
  Public testnet remains TN10 @ 10 BPS. This crate does not IBD TN12.

This encoding asks whether `tps` transactions per second can be packed into
`bps` block slots per second with at most `cap` transactions per block,
over a finite window of `window` seconds.

    feasible  iff  tps * window  <=  bps * window * cap
                iff  tps <= bps * cap

Default sweep reduces by gcd so the CNF stays small (same inequality):
  100 TPS @ 10 BPS → 10 txs into 1 slot. SAT iff cap >= 10.
Unreduced 100 pigeons into 10 holes is PHP-hard; pass --full only if you want that.

SAT at 100 TPS with cap=10 and bps=10 is that arithmetic (10 tx/block at
10 BPS). It is not a consensus proof, not a kaspad patch, and not 100 BPS.

Mass packing is a separate closed form: Crescendo max block mass 500_000
grams and a ~2k-mass P2PK transfer leave hundreds of tx/block, so 100 TPS
fits mass with room. Rothschild's 100 is an experiment load cap, not a
mass theorem.
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

# rusty-kaspa consensus (Crescendo / TN10). Not a TN12-specific constant.
MAX_BLOCK_MASS = 500_000
# Typical simple P2PK transfer mass; covenants are heavier.
SIMPLE_TX_MASS = 2_000
ROTHSCHILD_TPS = 5
ROTHSCHILD_TPS_CAP = 100
LIVE_BPS = 10


class CNF:
    def __init__(self) -> None:
        self.var: dict[str, int] = {}
        self.next = 1
        self.clauses: list[list[int]] = []

    def v(self, name: str) -> int:
        if name not in self.var:
            self.var[name] = self.next
            self.next += 1
        return self.var[name]

    def add(self, c: list[int]) -> None:
        self.clauses.append(c)

    def at_most(self, lits: list[int], k: int, tag: str) -> None:
        """Sinz sequential counter: at most k of lits are true."""
        n = len(lits)
        if k < 0:
            for lit in lits:
                self.add([-lit])
            return
        if k >= n:
            return
        if k == 0:
            for lit in lits:
                self.add([-lit])
            return
        if n == 0:
            return

        def s(i: int, j: int) -> int:
            return self.v(f"{tag}_s_{i}_{j}")

        self.add([-lits[0], s(0, 1)])
        for j in range(2, k + 1):
            self.add([-s(0, j)])
        for i in range(1, n - 1):
            self.add([-lits[i], s(i, 1)])
            self.add([-s(i - 1, 1), s(i, 1)])
            for j in range(2, k + 1):
                self.add([-lits[i], -s(i - 1, j - 1), s(i, j)])
                self.add([-s(i - 1, j), s(i, j)])
            self.add([-lits[i], -s(i - 1, k)])
        self.add([-lits[n - 1], -s(n - 2, k)])

    def exactly_one(self, lits: list[int], tag: str) -> None:
        if not lits:
            self.add([])
            return
        self.add(list(lits))
        self.at_most(lits, 1, tag)

    def write(self, path: Path, comments: list[str]) -> None:
        nv = self.next - 1
        lines = [f"c {c}" for c in comments]
        lines.append(f"p cnf {nv} {len(self.clauses)}")
        for c in self.clauses:
            if not c:
                lines.append("0")
            else:
                lines.append(" ".join(map(str, c)) + " 0")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def n_tx(tps: int, window_s: int) -> int:
    return tps * window_s


def n_slots(bps: int, window_s: int) -> int:
    return bps * window_s


def packing_fits(tps: int, bps: int, cap: int, window_s: int) -> bool:
    return n_tx(tps, window_s) <= n_slots(bps, window_s) * cap


def reduce_rate(tps: int, bps: int) -> tuple[int, int, int]:
    """Same tps <= bps * cap inequality, fewer pigeons. gcd(100,10)=10 → 10 tx / 1 slot."""
    if tps < 0 or bps <= 0:
        raise ValueError("tps must be >= 0 and bps > 0")
    g = math.gcd(tps, bps)
    if g == 0:
        g = 1
    return tps // g, bps // g, g


def mass_tx_per_block(max_mass: int = MAX_BLOCK_MASS, tx_mass: int = SIMPLE_TX_MASS) -> int:
    if tx_mass <= 0:
        raise ValueError("tx_mass must be positive")
    return max_mass // tx_mass


def encode_packing(tps: int, bps: int, cap: int, window_s: int) -> tuple[CNF, dict[str, int]]:
    if min(tps, bps, cap, window_s) < 0:
        raise ValueError("tps, bps, cap, window_s must be non-negative")
    if bps == 0 or window_s == 0:
        raise ValueError("bps and window_s must be positive")
    cnf = CNF()
    txs = n_tx(tps, window_s)
    slots = n_slots(bps, window_s)
    assign: dict[str, int] = {}
    for t in range(txs):
        row = [cnf.v(f"x_{t}_{s}") for s in range(slots)]
        for s, lit in enumerate(row):
            assign[f"x_{t}_{s}"] = lit
        cnf.exactly_one(row, f"tx{t}")
    for s in range(slots):
        col = [cnf.v(f"x_{t}_{s}") for t in range(txs)]
        cnf.at_most(col, cap, f"slot{s}")
    return cnf, assign


def decode_model(
    model: list[int],
    tps: int,
    bps: int,
    window_s: int,
    assign: dict[str, int],
) -> list[int]:
    """Return per-slot transaction counts from a SAT model."""
    pos = {lit for lit in model if lit > 0}
    slots = n_slots(bps, window_s)
    counts = [0] * slots
    for t in range(n_tx(tps, window_s)):
        for s in range(slots):
            if assign[f"x_{t}_{s}"] in pos:
                counts[s] += 1
    return counts


def solve_cnf(clauses: list[list[int]], _nvars: int | None = None) -> tuple[str, list[int] | None]:
    try:
        from pysat.solvers import Glucose4
    except ImportError:
        return "no-pysat", None
    with Glucose4(bootstrap_with=clauses) as solver:
        if solver.solve():
            model = solver.get_model() or []
            return "SAT", model
        return "UNSAT", None


def instance_name(tps: int, bps: int, cap: int, window_s: int) -> str:
    return f"rothschild_tps{tps}_bps{bps}_cap{cap}_w{window_s}"


def run_instance(
    tps: int,
    bps: int,
    cap: int,
    window_s: int,
    out_dir: Path,
    *,
    reduce: bool = True,
) -> dict:
    orig_tps, orig_bps = tps, bps
    g = 1
    if reduce:
        tps, bps, g = reduce_rate(tps, bps)
    fits = packing_fits(tps, bps, cap, window_s)
    cnf, assign = encode_packing(tps, bps, cap, window_s)
    name = instance_name(tps, bps, cap, window_s)
    if reduce and g > 1:
        name = f"{name}_gcd{g}"
    path = out_dir / f"{name}.cnf"
    cnf.write(
        path,
        [
            "TN12 Rothschild TPS packing. TPS != BPS. Not 100 BPS.",
            f"rothschild_tps={orig_tps} live_bps={orig_bps} gcd={g}",
            f"encoded tps={tps} bps={bps} cap={cap} window_s={window_s}",
            f"txs={n_tx(tps, window_s)} slots={n_slots(bps, window_s)}",
            f"arithmetic_fits={fits}",
        ],
    )
    status, model = solve_cnf(cnf.clauses)
    expected = "SAT" if fits else "UNSAT"
    counts = None
    if status == "SAT" and model is not None:
        counts = decode_model(model, tps, bps, window_s, assign)
        if max(counts, default=0) > cap:
            status = "decode-cap-violation"
    row = {
        "name": name,
        "rothschild_tps": orig_tps,
        "live_bps": orig_bps,
        "gcd": g,
        "tps": tps,
        "bps": bps,
        "cap": cap,
        "window_s": window_s,
        "txs": n_tx(tps, window_s),
        "slots": n_slots(bps, window_s),
        "arithmetic": expected,
        "solver": status,
        "nvars": cnf.next - 1,
        "nclauses": len(cnf.clauses),
        "cnf": str(path),
        "slot_counts": counts,
        "match": status == expected,
        "note": "toy packing; not consensus; TN12 is covenants not 100 BPS",
    }
    return row


def sweep_rows(
    bps: int, window_s: int, out_dir: Path, *, reduce: bool = True
) -> list[dict]:
    specs = [
        (ROTHSCHILD_TPS, 1),
        (ROTHSCHILD_TPS_CAP, 1),
        (ROTHSCHILD_TPS_CAP, 9),
        (ROTHSCHILD_TPS_CAP, 10),
    ]
    return [
        run_instance(tps, bps, cap, window_s, out_dir, reduce=reduce)
        for tps, cap in specs
    ]


def print_what_is_tps() -> None:
    print("TPS = transactions per second (Rothschild -t).", flush=True)
    print("BPS = blocks per second (live L1 is GHOSTDAG @ 10 BPS).", flush=True)
    print("They are not the same unit. 100 TPS is not 100 BPS.", flush=True)
    print("TN12 Rothschild: -t=5 recommended; do not go above 100 TPS.", flush=True)
    print("TN12 is the covenants testnet, not a 100 BPS net.", flush=True)
    print(f"Packing bound: tps <= bps * cap   (here bps={LIVE_BPS}).", flush=True)
    print(
        f"Mass room (not Rothschild): max {MAX_BLOCK_MASS} / ~{SIMPLE_TX_MASS} "
        f"= {mass_tx_per_block()} tx/block "
        f"-> ~{LIVE_BPS * mass_tx_per_block()} TPS before mass fills.",
        flush=True,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Encode/solve Rothschild 5 vs 100 TPS packing. TPS != BPS."
    )
    parser.add_argument("--tps", type=int, default=None, help="transactions per second")
    parser.add_argument("--bps", type=int, default=LIVE_BPS, help="blocks per second (default 10)")
    parser.add_argument("--cap", type=int, default=None, help="max transactions per block")
    parser.add_argument("--window", type=int, default=1, help="window seconds (default 1)")
    parser.add_argument(
        "--out",
        type=Path,
        default=Path(__file__).resolve().parent / "tn12_tps_out",
        help="directory for DIMACS files",
    )
    parser.add_argument(
        "--sweep",
        action="store_true",
        help="Rothschild 5 TPS cap=1 and 100 TPS cap=1/9/10",
    )
    parser.add_argument(
        "--full",
        action="store_true",
        help="do not gcd-reduce; 100 txs into 10 slots is PHP-hard",
    )
    parser.add_argument(
        "--what-is-tps",
        action="store_true",
        help="print TPS vs BPS and exit",
    )
    args = parser.parse_args(argv)

    if args.what_is_tps:
        print_what_is_tps()
        return 0

    print_what_is_tps()
    print(flush=True)

    reduce = not args.full
    if args.sweep or (args.tps is None and args.cap is None):
        rows = sweep_rows(args.bps, args.window, args.out, reduce=reduce)
    else:
        tps = ROTHSCHILD_TPS_CAP if args.tps is None else args.tps
        cap = 10 if args.cap is None else args.cap
        rows = [run_instance(tps, args.bps, cap, args.window, args.out, reduce=reduce)]

    print(
        f"{'r_tps':>5} {'enc':>12} {'cap':>4} {'txs':>5} {'slots':>5} "
        f"{'arith':>6} {'solver':>10} match",
        flush=True,
    )
    for row in rows:
        enc = f"{row['tps']}/{row['bps']}"
        print(
            f"{row['rothschild_tps']:5d} {enc:>12} {row['cap']:4d} {row['txs']:5d} "
            f"{row['slots']:5d} {row['arithmetic']:>6} {row['solver']:>10} "
            f"{str(row['match']).lower()}",
            flush=True,
        )
    report = args.out / "sweep.json"
    report.write_text(json.dumps(rows, indent=2) + "\n", encoding="utf-8")
    print(flush=True)
    print(f"wrote {report}", flush=True)
    if any(r["solver"] == "no-pysat" for r in rows):
        print("solver skipped: pip install python-sat", file=sys.stderr)
        return 2
    if any(not r["match"] for r in rows):
        print("solver disagreed with arithmetic", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
