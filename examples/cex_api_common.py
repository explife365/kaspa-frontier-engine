"""CEX + DEX integrator API payloads and scenario validation."""

from __future__ import annotations

import os
import sys
import time
from pathlib import Path
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from dev_quickstart import PATHS, build_card  # noqa: E402
from integrator_status import FIXTURES, build_report, verify_fixture_offline  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402
from tn10_wallets import ensure_wallets  # noqa: E402


def cex_wallets(root: Path | None = None) -> dict[str, Any]:
    load_kaspa_env(root or ROOT)
    wallets = ensure_wallets(root)
    return {
        "network": "testnet-10",
        "not_consensus": True,
        "count": len(wallets),
        "wallets": [
            {"name": w.name, "address": w.address, "explorer": w.explorer} for w in wallets
        ],
    }


def dex_pairs() -> dict[str, Any]:
    from galleon_dex_common import PAIR_TOKEN0, PAIR_TOKEN1, dex_status, pool_kind

    load_kaspa_env(ROOT)
    status = dex_status()
    p = status["pair"]
    return {
        "ok": True,
        "pool_kind": pool_kind(),
        "pool": status["pool"],
        "pairs": [
            {
                "id": "gTEST-wiKAS",
                "token0": p["token0"],
                "token1": p["token1"],
                "reserves": status["reserves"],
                "price_token1_per_token0": status["price_token1_per_token0"],
                "fee_bps": status["fee_bps"],
            }
        ],
        "token0": PAIR_TOKEN0,
        "token1": PAIR_TOKEN1,
    }


def cex_readiness(skip_gate: bool = False) -> dict[str, Any]:
    report = build_report(skip_gate)
    dex_ok = False
    dex_error: str | None = None
    try:
        from galleon_dex_common import dex_status

        dex_status()
        dex_ok = True
    except Exception as err:  # noqa: BLE001
        dex_error = str(err)
    gate = report.get("gate") or {}
    fixtures = report.get("fixtures") or {}
    sdk = report.get("sdk") or {}
    return {
        "network": "testnet-10",
        "not_consensus": True,
        "ready_for_cex_demo": bool(report.get("ready_for_cex_demo")),
        "ready_for_dex_quotes": dex_ok,
        "checks": {
            "adoption_gate": bool(gate.get("adoption_ok")),
            "fixtures_offline": bool(fixtures.get("all_ok")),
            "sdk_native": bool(sdk.get("readyNative")),
            "sdk_dev_patch": bool(sdk.get("readyWithDevPatch")),
            "dex_pool": dex_ok,
        },
        "gate": gate,
        "dex_error": dex_error,
        "galleon": report.get("galleon"),
    }


def onboard_card(path_id: str) -> dict[str, Any]:
    if path_id not in PATHS:
        raise ValueError(f"unknown path: {path_id}; use rest|galleon|nodes|covenant")
    card = build_card(path_id)
    card["ok"] = True
    return card


def run_scenario(
    name: str,
    fn: Callable[[], dict[str, Any]],
    *,
    require_ok: bool = True,
) -> dict[str, Any]:
    started = time.perf_counter()
    try:
        body = fn()
        ok = bool(body.get("ok", True)) if require_ok else True
        if require_ok and "error" in body and body["error"]:
            ok = False
    except Exception as err:  # noqa: BLE001
        body = {"error": str(err)}
        ok = False
    return {
        "scenario": name,
        "ok": ok,
        "latency_ms": int((time.perf_counter() - started) * 1000),
        "body": body,
    }


def validate_scenarios(
    *,
    skip_gate: bool = True,
    live_dex: bool = True,
) -> dict[str, Any]:
    load_kaspa_env(ROOT)
    scenarios: list[dict[str, Any]] = []

    scenarios.append(
        run_scenario(
            "fixtures_offline",
            lambda: {
                "ok": all(verify_fixture_offline(n).get("ok") for n in FIXTURES),
                "count": len(FIXTURES),
            },
        )
    )
    scenarios.append(
        run_scenario(
            "cex_wallets",
            lambda: {"ok": True, **cex_wallets()},
            require_ok=False,
        )
    )
    scenarios.append(
        run_scenario(
            "onboard_nodes",
            lambda: onboard_card("nodes"),
            require_ok=False,
        )
    )
    scenarios.append(
        run_scenario(
            "cex_readiness",
            lambda: {"ok": True, **cex_readiness(skip_gate=skip_gate)},
            require_ok=False,
        )
    )

    if live_dex and (os.environ.get("GALLEON_MINI_POOL") or os.environ.get("GALLEON_FEE_POOL")):
        from galleon_dex_common import dex_quote

        scenarios.append(
            run_scenario("dex_status", lambda: {"ok": True, **dex_pairs()}, require_ok=False)
        )
        scenarios.append(
            run_scenario(
                "dex_quote_gTEST_to_wiKAS",
                lambda: {"ok": True, **dex_quote(1.0, True)},
                require_ok=False,
            )
        )
        scenarios.append(
            run_scenario(
                "dex_quote_wiKAS_to_gTEST",
                lambda: {"ok": True, **dex_quote(0.001, False)},
                require_ok=False,
            )
        )
        from galleon_dex_common import dex_swap_plan

        scenarios.append(
            run_scenario(
                "dex_swap_dry_run",
                lambda: {"ok": True, **dex_swap_plan(0.5, "wiKAS")},
                require_ok=False,
            )
        )

    ok_count = sum(1 for s in scenarios if s["ok"])
    return {
        "not_consensus": True,
        "generated_at": int(time.time()),
        "ok": ok_count == len(scenarios),
        "passed": ok_count,
        "total": len(scenarios),
        "scenarios": scenarios,
    }
