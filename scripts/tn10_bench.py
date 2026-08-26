"""Live TN10 REST benchmark: sequential vs concurrent snapshots.

  python scripts/tn10_bench.py
"""

from __future__ import annotations

import json
import statistics
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from tn10_rest import REST_BASE, encode_path_segment, get_json  # noqa: E402

ROUNDS = 7
ALICE = "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt"
TXIDS = [
    "6d0acd6fcbaf68bca1568a3cbbafe0f3c1d72c4f6ea0edc6f6c013a59cb5d591",
    "796445b99363dee7540d7dd75287c5a980d110bab007cee8d913ae254541410d",
    "480fc61819f73dc476cbc5b12fb974bd711457f07e4d07b074ca19bdb77a5dcd",
]


def timed(fn):
    start = time.perf_counter()
    value = fn()
    ms = (time.perf_counter() - start) * 1000.0
    return ms, value


def fetch(path: str):
    return get_json(path)


def sequential(paths: list[str]) -> None:
    for path in paths:
        fetch(path)


def concurrent(paths: list[str]) -> None:
    with ThreadPoolExecutor(max_workers=len(paths)) as pool:
        futures = [pool.submit(fetch, path) for path in paths]
        for fut in as_completed(futures):
            fut.result()


def summarize(samples: list[float]) -> dict:
    ordered = sorted(samples)
    n = len(ordered)
    p95_idx = min(n - 1, max(0, int(round(0.95 * (n - 1)))))
    return {
        "n": n,
        "min_ms": round(ordered[0], 1),
        "median_ms": round(statistics.median(ordered), 1),
        "p95_ms": round(ordered[p95_idx], 1),
        "mean_ms": round(statistics.mean(ordered), 1),
        "max_ms": round(ordered[-1], 1),
        "samples_ms": [round(x, 1) for x in samples],
    }


def run_scenario(name: str, fn, rounds: int = ROUNDS) -> dict:
    timed(fn)  # warmup / TLS session
    samples: list[float] = []
    errors = 0
    for _ in range(rounds):
        try:
            ms, _ = timed(fn)
            samples.append(ms)
        except Exception:
            errors += 1
    if not samples:
        return {"name": name, "ok": 0, "errors": errors}
    out = summarize(samples)
    out["name"] = name
    out["ok"] = len(samples)
    out["errors"] = errors
    return out


def reliability_probe() -> dict:
    import urllib.error
    import urllib.request

    url = f"{REST_BASE}/info/network"
    no_ua = urllib.request.Request(url, headers={"Accept": "application/json"})
    with_ua = urllib.request.Request(
        url,
        headers={"User-Agent": "kaspa-frontier-engine/0.3", "Accept": "application/json"},
    )
    def status(req) -> int:
        try:
            with urllib.request.urlopen(req, timeout=12) as resp:
                return int(resp.status)
        except urllib.error.HTTPError as err:
            return int(err.code)

    return {
        "no_user_agent": status(no_ua),
        "with_user_agent": status(with_ua),
    }


def main() -> None:
    status_paths = ["/info/blockdag", "/info/hashrate"]
    enc = encode_path_segment(ALICE)
    address_paths = [
        "/info/blockdag",
        f"/addresses/{enc}/utxos",
        f"/addresses/{enc}/balance",
    ]
    tx_paths = [f"/transactions/{txid}" for txid in TXIDS]

    results = [
        run_scenario("status sequential (dag+hashrate)", lambda: sequential(status_paths)),
        run_scenario("status concurrent (dag+hashrate)", lambda: concurrent(status_paths)),
        run_scenario("address sequential (dag+utxos+balance)", lambda: sequential(address_paths)),
        run_scenario("address concurrent (dag+utxos+balance)", lambda: concurrent(address_paths)),
        run_scenario("proof sequential (3 txs)", lambda: sequential(tx_paths)),
        run_scenario("proof concurrent (3 txs)", lambda: concurrent(tx_paths)),
    ]
    rel = reliability_probe()
    payload = {
        "endpoint": REST_BASE,
        "rounds": ROUNDS,
        "when": time.strftime("%Y-%m-%d %H:%M UTC", time.gmtime()),
        "reliability": rel,
        "scenarios": results,
    }
    print(json.dumps(payload, indent=2))


if __name__ == "__main__":
    main()
