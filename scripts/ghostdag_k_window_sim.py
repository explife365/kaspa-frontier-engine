#!/usr/bin/env python3
"""In-process delay-window occupancy vs live GHOSTDAG k.

Not kaspad. Not a fork. Not DAGKnight. Not 100 BPS activation.

N virtual miners emit at a shared target BPS. Each block reaches a single
observer after a drawn one-way delay. The observer's sliding window of
length L is the same toy occupancy the SAT packing used:

    concurrent ≈ bps * L
    GHOSTDAG k covers the window iff occupancy <= k

Default L=1s, k=18:
  10 BPS  → occupancy ~10  (slack)
  100 BPS → occupancy ~100 (over k)

This is a configuration probe for that inequality, not rusty-kaspa consensus.
Do not IBD a homemade net. Do not patch kaspad.
"""
from __future__ import annotations

import argparse
import json
import random
from dataclasses import asdict, dataclass
from pathlib import Path

LIVE_BPS = 10
LORE_BPS = 100
GHOSTDAG_K = 18
DEFAULT_LATENCY_S = 1.0
DEFAULT_MINERS = 4
DEFAULT_SECONDS = 20.0
DEFAULT_SEED = 10


@dataclass(frozen=True)
class OccupancyReport:
    bps: int
    miners: int
    seconds: float
    latency_s: float
    k: int
    seed: int
    blocks: int
    max_occupancy: int
    mean_occupancy: float
    p_over_k: float
    covers: bool
    note: str


def emit_times(bps: int, seconds: float, rng: random.Random) -> list[float]:
    """Deterministic Poisson-like emissions: one block every 1/bps plus jitter."""
    if bps <= 0 or seconds <= 0:
        return []
    interval = 1.0 / bps
    times: list[float] = []
    t = interval * rng.random()
    while t < seconds:
        times.append(t)
        t += interval * (0.85 + 0.3 * rng.random())
    return times


def receive_times(
    emits: list[float], miners: int, latency_s: float, rng: random.Random
) -> list[float]:
    if miners < 1:
        raise ValueError("miners must be >= 1")
    received: list[float] = []
    for i, t in enumerate(emits):
        delay = latency_s * (0.5 + rng.random()) if latency_s > 0 else 0.0
        _miner = i % miners
        del _miner
        received.append(t + delay)
    received.sort()
    return received


def window_occupancy(received: list[float], latency_s: float) -> list[int]:
    """Occupancy of (t-L, t] at each receive time t."""
    if not received:
        return []
    counts: list[int] = []
    left = 0
    for right, t in enumerate(received):
        cutoff = t - latency_s
        while left <= right and received[left] <= cutoff:
            left += 1
        counts.append(right - left + 1)
    return counts


def simulate(
    *,
    bps: int,
    miners: int = DEFAULT_MINERS,
    seconds: float = DEFAULT_SECONDS,
    latency_s: float = DEFAULT_LATENCY_S,
    k: int = GHOSTDAG_K,
    seed: int = DEFAULT_SEED,
) -> OccupancyReport:
    rng = random.Random(seed)
    emits = emit_times(bps, seconds, rng)
    received = receive_times(emits, miners, latency_s, rng)
    occ = window_occupancy(received, latency_s)
    max_occ = max(occ) if occ else 0
    mean_occ = sum(occ) / len(occ) if occ else 0.0
    over = sum(1 for n in occ if n > k)
    p_over = over / len(occ) if occ else 0.0
    return OccupancyReport(
        bps=bps,
        miners=miners,
        seconds=seconds,
        latency_s=latency_s,
        k=k,
        seed=seed,
        blocks=len(received),
        max_occupancy=max_occ,
        mean_occupancy=round(mean_occ, 3),
        p_over_k=round(p_over, 4),
        covers=max_occ <= k,
        note="toy delay-window occupancy; not kaspad; not DAGKnight; live remains GHOSTDAG @ 10 BPS",
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Virtual miners vs GHOSTDAG k occupancy. Not a kaspad fork."
    )
    parser.add_argument("--miners", type=int, default=DEFAULT_MINERS)
    parser.add_argument("--seconds", type=float, default=DEFAULT_SECONDS)
    parser.add_argument("--latency", type=float, default=DEFAULT_LATENCY_S)
    parser.add_argument("--k", type=int, default=GHOSTDAG_K)
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    parser.add_argument(
        "--sweep-k",
        action="store_true",
        help="at 100 BPS, try k in 18,50,100,128 (jitter can exceed bps*L)",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path(__file__).resolve().parent.parent / ".local" / "k_window_sim.json",
    )
    args = parser.parse_args()
    print("Not kaspad. Not DAGKnight. Occupancy vs live GHOSTDAG k only.", flush=True)
    if args.sweep_k:
        return _print_rows(
            [
                asdict(
                    simulate(
                        bps=LORE_BPS,
                        miners=args.miners,
                        seconds=args.seconds,
                        latency_s=args.latency,
                        k=k,
                        seed=args.seed + 1,
                    )
                )
                for k in (18, 50, 100, 128)
            ],
            args.out,
            k_column=True,
            expect=lambda rows: (not rows[0]["covers"]) and rows[-1]["covers"],
        )
    rows = [
        asdict(simulate(bps=LIVE_BPS, miners=args.miners, seconds=args.seconds, latency_s=args.latency, k=args.k, seed=args.seed)),
        asdict(simulate(bps=LORE_BPS, miners=args.miners, seconds=args.seconds, latency_s=args.latency, k=args.k, seed=args.seed + 1)),
    ]
    return _print_rows(
        rows,
        args.out,
        k_column=False,
        expect=lambda rows: rows[0]["covers"] and not rows[1]["covers"],
    )


def _print_rows(rows: list[dict], out: Path, *, k_column: bool, expect) -> int:
    if k_column:
        print(
            f"{'bps':>4} {'k':>4} {'n':>5} {'max':>4} {'mean':>7} {'P(>k)':>7} covers",
            flush=True,
        )
        for row in rows:
            print(
                f"{row['bps']:4d} {row['k']:4d} {row['blocks']:5d} {row['max_occupancy']:4d} "
                f"{row['mean_occupancy']:7.2f} {row['p_over_k']:7.3f} {str(row['covers']).lower()}",
                flush=True,
            )
    else:
        print(f"{'bps':>4} {'n':>4} {'max':>4} {'mean':>7} {'P(>k)':>7} covers", flush=True)
        for row in rows:
            print(
                f"{row['bps']:4d} {row['blocks']:4d} {row['max_occupancy']:4d} "
                f"{row['mean_occupancy']:7.2f} {row['p_over_k']:7.3f} {str(row['covers']).lower()}",
                flush=True,
            )
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(rows, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {out}", flush=True)
    if not expect(rows):
        print("unexpected occupancy vs k; check seed/params", flush=True)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
