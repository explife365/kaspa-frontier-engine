#!/usr/bin/env python3
"""100 BPS lore vs live GHOSTDAG k — toy CNF.

BPS = blocks per second. Not TPS (transactions per second).

Live L1 is GHOSTDAG @ 10 BPS (Crescendo k=18). kaspa.org lore: 100 BPS is a
later hard fork, not TN12, not this crate's telemetry. KIP-2 / DAGKnight is
Proposed. SAT here does not activate it. Do not patch kaspad.

Toy question: in a delay window of L seconds, about bps*L honest blocks can
show up concurrently. GHOSTDAG's k-cluster bound is k. Packing fits iff

    bps * L  <=  k

Default L=1s, k=18:
  10 BPS  → 10 concurrent  ≤ 18  SAT   (live slack)
  100 BPS → 100 concurrent > 18  UNSAT (k=18 cannot cover a 1s WAN burst)

The 100 BPS instance encodes k+1=19 pigeons into 18 holes (one-over is
enough). That is why 100 BPS is research (KIP-2), not a number to print as
live DAA/s.

This crate keeps TARGET_BPS = 10.0.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from tn12_tps_cnf import encode_packing, solve_cnf

LIVE_BPS = 10
LORE_BPS = 100
GHOSTDAG_K = 18
DEFAULT_LATENCY_S = 1


def concurrent_blocks(bps: int, latency_s: int) -> int:
    return bps * latency_s


def k_covers(bps: int, k: int, latency_s: int) -> bool:
    return concurrent_blocks(bps, latency_s) <= k


def encode_bps_vs_k(
    bps: int, k: int, latency_s: int, *, full_n: bool = False
) -> tuple:
    """n concurrent blocks into one delay window with capacity k.

    Default UNSAT uses k+1 pigeons (same UNSAT, small CNF).
    --full-n encodes bps*L pigeons (harder; host02 grind).
    """
    n = concurrent_blocks(bps, latency_s)
    encoded_n = n if (full_n or n <= k) else k + 1
    return encode_packing(encoded_n, 1, k, 1), encoded_n, n


def run_instance(
    bps: int, k: int, latency_s: int, out_dir: Path, *, full_n: bool = False
) -> dict:
    fits = k_covers(bps, k, latency_s)
    (cnf, assign), encoded_n, n = encode_bps_vs_k(bps, k, latency_s, full_n=full_n)
    name = f"ghostdag_k{k}_bps{bps}_L{latency_s}"
    if full_n and encoded_n > k:
        name = f"{name}_fulln"
    path = out_dir / f"{name}.cnf"
    cnf.write(
        path,
        [
            "100 BPS lore vs live GHOSTDAG k. Not live telemetry. Not KIP-2.",
            f"bps={bps} k={k} latency_s={latency_s} concurrent={n} encoded_n={encoded_n}",
            f"arithmetic_fits={fits}",
            "TARGET_BPS stays 10. Do not emit 100 as live DAA/s.",
        ],
    )
    status, _model = ("host02", None) if (full_n and encoded_n > 40) else solve_cnf(cnf.clauses)
    expected = "SAT" if fits else "UNSAT"
    return {
        "name": name,
        "bps": bps,
        "k": k,
        "latency_s": latency_s,
        "concurrent": n,
        "encoded_n": encoded_n,
        "arithmetic": expected,
        "solver": status,
        "nvars": cnf.next - 1,
        "nclauses": len(cnf.clauses),
        "cnf": str(path),
        "match": status == expected or status == "host02",
        "note": "toy k-window; not consensus; live remains GHOSTDAG @ 10 BPS",
        "assign_vars": len(assign),
    }


def sweep_rows(out_dir: Path, latency_s: int) -> list[dict]:
    return [
        run_instance(LIVE_BPS, GHOSTDAG_K, latency_s, out_dir),
        run_instance(LORE_BPS, GHOSTDAG_K, latency_s, out_dir),
        run_instance(LORE_BPS, LORE_BPS, latency_s, out_dir),
    ]


def print_what_is_100bps() -> None:
    print("BPS = blocks per second. Live L1 is GHOSTDAG @ 10 BPS.", flush=True)
    print("100 BPS is kaspa.org lore for a later HF (KIP-2 / DAGKnight).", flush=True)
    print("It is not TN12, not Rothschild TPS, and not this node's telemetry.", flush=True)
    print(
        f"GHOSTDAG k={GHOSTDAG_K}: delay-window packing fits iff bps * L <= k.",
        flush=True,
    )
    print("SAT at 10 BPS does not activate 100 BPS. UNSAT at 100 BPS / k=18", flush=True)
    print("is why k must change (research), not a number to fake on explorers.", flush=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Encode/solve 10 vs 100 BPS against GHOSTDAG k. Not live telemetry."
    )
    parser.add_argument("--bps", type=int, default=None)
    parser.add_argument("--k", type=int, default=GHOSTDAG_K)
    parser.add_argument("--latency", type=int, default=DEFAULT_LATENCY_S)
    parser.add_argument(
        "--out",
        type=Path,
        default=Path(__file__).resolve().parent / "bps100_out",
    )
    parser.add_argument("--sweep", action="store_true")
    parser.add_argument(
        "--full-n",
        action="store_true",
        help="encode bps*L pigeons (host02 grind); default reduces UNSAT to k+1",
    )
    parser.add_argument("--what-is-100bps", action="store_true")
    args = parser.parse_args(argv)

    if args.what_is_100bps:
        print_what_is_100bps()
        return 0

    print_what_is_100bps()
    print(flush=True)

    if args.sweep or args.bps is None:
        rows = sweep_rows(args.out, args.latency)
    else:
        rows = [run_instance(args.bps, args.k, args.latency, args.out, full_n=args.full_n)]

    print(
        f"{'bps':>4} {'k':>4} {'L':>3} {'n':>4} {'enc':>4} {'arith':>6} {'solver':>10} match",
        flush=True,
    )
    for row in rows:
        print(
            f"{row['bps']:4d} {row['k']:4d} {row['latency_s']:3d} {row['concurrent']:4d} "
            f"{row['encoded_n']:4d} {row['arithmetic']:>6} {row['solver']:>10} "
            f"{str(row['match']).lower()}",
            flush=True,
        )
    report = args.out / "sweep.json"
    args.out.mkdir(parents=True, exist_ok=True)
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
