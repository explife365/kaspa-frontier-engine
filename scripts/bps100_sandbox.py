#!/usr/bin/env python3
"""Local 100 BPS packing sandbox. Not kaspad. Not a testnet. Not activation.

Proves the delay-window function:
    occupancy ≈ bps * L
    live GHOSTDAG k=18 covers iff occupancy <= k

Run:
    python scripts/bps100_sandbox.py
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "scripts"


def main() -> int:
    print("SANDBOX — not kaspad, not DAGKnight, not live 100 BPS", flush=True)
    print("Live L1 stays GHOSTDAG @ 10 BPS k=18.", flush=True)
    print("SAT packing is exact bps*L. Occupancy jitter can exceed that.", flush=True)
    print(flush=True)
    occupancy = [
        sys.executable,
        str(SCRIPTS / "ghostdag_k_window_sim.py"),
        "--miners",
        "4",
        "--seconds",
        "8",
    ]
    sweep = occupancy + [
        "--sweep-k",
        "--out",
        str(ROOT / ".local" / "k_window_sim_sweep.json"),
    ]
    sat = [sys.executable, str(SCRIPTS / "bps100_cnf.py"), "--sweep"]
    print("== occupancy 10 vs 100 BPS at k=18 ==", flush=True)
    live = subprocess.call(occupancy, cwd=ROOT)
    print(flush=True)
    print("== occupancy 100 BPS while k scales ==", flush=True)
    scaled = subprocess.call(sweep, cwd=ROOT)
    print(flush=True)
    print("== SAT packing 10/k18, 100/k18, 100/k100 ==", flush=True)
    packing = subprocess.call(sat, cwd=ROOT)
    if live or scaled:
        return live or scaled
    if packing == 2:
        print("SAT solver optional here; occupancy already ran.", flush=True)
        return 0
    return packing


if __name__ == "__main__":
    raise SystemExit(main())
