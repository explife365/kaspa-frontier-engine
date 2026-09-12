"""TN10 + Galleon integrator rehearsal status (one command).

  python examples/integrator_status.py
  python examples/integrator_status.py --json
  python examples/integrator_status.py --skip-gate   # faster: no cargo node-health

Aggregates adoption gate, SDK gate, offline fixture proofs, L1/L2 bridge state,
and Galleon pool / wallet balances. Not consensus evidence.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from kaspa_env import load_kaspa_env  # noqa: E402

FIXTURES = (
    "counter",
    "vault",
    "swap",
    "htlc",
    "htlc-refund",
    "htlc-sha256",
    "htlc-sha256-refund",
    "escrow-2of3",
)

SHA256_REFUND_FIXTURE = ROOT / "fixtures" / "tn10-htlc-sha256-refund-proof.json"


def _run_json(cmd: list[str], *, cwd: Path = ROOT, timeout: int = 180) -> dict[str, Any]:
    try:
        proc = subprocess.run(
            cmd,
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except (subprocess.TimeoutExpired, OSError) as err:
        return {"ok": False, "error": str(err)}
    text = (proc.stdout or proc.stderr or "").strip()
    if not text:
        return {"ok": proc.returncode == 0, "exitCode": proc.returncode}
    try:
        body = json.loads(text)
    except json.JSONDecodeError:
        return {"ok": False, "exitCode": proc.returncode, "raw": text[:500]}
    if isinstance(body, dict):
        body.setdefault("ok", proc.returncode == 0)
        body["exitCode"] = proc.returncode
    return body


def verify_fixture_offline(name: str) -> dict[str, Any]:
    path = ROOT / "fixtures" / f"tn10-{name}-proof.json"
    if not path.is_file():
        return {"fixture": name, "ok": False, "error": "missing proof file"}
    proc = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "--release",
            "--bin",
            "tn10-proof",
            "--",
            str(path.relative_to(ROOT)),
            "--offline",
            "--json",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    )
    text = proc.stdout.strip()
    if proc.returncode != 0:
        return {
            "fixture": name,
            "ok": False,
            "exitCode": proc.returncode,
            "error": (proc.stderr or text or "offline verify failed")[:300],
        }
    try:
        body = json.loads(text)
    except json.JSONDecodeError:
        return {"fixture": name, "ok": False, "error": "json parse failed"}
    return {
        "fixture": name,
        "ok": True,
        "covenant_id": body.get("covenant_id"),
        "kascov_url": body.get("kascov_url"),
    }


def adoption_card(skip_gate: bool) -> dict[str, Any]:
    cmd = [sys.executable, str(ROOT / "scripts" / "tn10_adoption_scorecard.py"), "--json"]
    if skip_gate:
        cmd.append("--skip-gate")
    return _run_json(cmd, timeout=150)


def sdk_gate() -> dict[str, Any]:
    env = os.environ.copy()
    env.setdefault("TN10_SDK_DEV_PATCH", "1")
    try:
        proc = subprocess.run(
            [sys.executable, str(ROOT / "scripts" / "tn10_sdk_gate.py"), "--json"],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            timeout=60,
            check=False,
        )
    except (subprocess.TimeoutExpired, OSError) as err:
        return {"ok": False, "error": str(err)}
    text = proc.stdout.strip()
    try:
        body = json.loads(text)
    except json.JSONDecodeError:
        return {"ok": False, "raw": text[:500]}
    body["ok"] = bool(body.get("readyWithDevPatch") or body.get("readyNative"))
    body["exitCode"] = proc.returncode
    return body


def galleon_wallet() -> dict[str, Any]:
    try:
        from galleon import GALLEON_RPC, GALLEON_WRAPPED_IKAS
        from galleon_faucet import address_of, galleon_key, rpc_hex
        from erc20 import token_balance

        owner = address_of(galleon_key())
        native = int(rpc_hex("eth_getBalance", [owner, "latest"]), 16)
        wikas = token_balance(GALLEON_RPC, GALLEON_WRAPPED_IKAS, owner) if GALLEON_WRAPPED_IKAS else 0
        return {
            "ok": True,
            "address": owner,
            "native_ikas": native / 1e18,
            "wikas": wikas / 1e18,
        }
    except Exception as err:  # noqa: BLE001 — status probe
        return {"ok": False, "error": str(err)}


def bridge_status() -> dict[str, Any]:
    out: dict[str, Any] = {"int_tag": None, "sha256": None}
    bridge_int = (os.environ.get("GALLEON_HTLC_BRIDGE") or "").strip()
    bridge_sha = (os.environ.get("GALLEON_HTLC_BRIDGE_SHA256") or "").strip()
    try:
        from galleon import GALLEON_RPC
        from l1_l2_bridge_release import status as status_int
        from l1_l2_bridge_release_sha256 import status as status_sha256

        if bridge_int:
            out["int_tag"] = status_int(GALLEON_RPC, bridge_int)
        if bridge_sha:
            body = status_sha256(GALLEON_RPC, bridge_sha)
            body["payout_ok"] = body.get("payout_amount") == 10**15
            out["sha256"] = body
    except Exception as err:  # noqa: BLE001
        out["error"] = str(err)
    return out


def pool_status() -> dict[str, Any]:
    pool = (os.environ.get("GALLEON_MINI_POOL") or "").strip()
    if not pool:
        return {"ok": False, "error": "missing GALLEON_MINI_POOL"}
    try:
        from galleon import GALLEON_GTEST, GALLEON_RPC, GALLEON_WRAPPED_IKAS
        from galleon_pool import get_reserves
        from erc20 import token_meta

        r0, r1 = get_reserves(GALLEON_RPC, pool)
        m0 = token_meta(GALLEON_RPC, GALLEON_GTEST)
        m1 = token_meta(GALLEON_RPC, GALLEON_WRAPPED_IKAS)
        d0 = m0.get("decimals", 18)
        d1 = m1.get("decimals", 18)
        return {
            "ok": True,
            "pool": pool,
            "reserves": {
                m0.get("symbol", "token0"): r0 / (10**d0),
                m1.get("symbol", "token1"): r1 / (10**d1),
            },
        }
    except Exception as err:  # noqa: BLE001
        return {"ok": False, "error": str(err)}


def timeout_playbook() -> dict[str, Any]:
    from l1_l2_htlc_bridge import playbook

    pb = playbook()
    sha256_refund_published = SHA256_REFUND_FIXTURE.is_file()
    return {
        "int_l1_refund_fixture": pb["l1"]["refund_fixture"],
        "int_l1_verify_refund": pb["l1"]["verify_refund"],
        "sha256_l1_refund_fixture": "fixtures/tn10-htlc-sha256-refund-proof.json",
        "sha256_l1_refund_published": sha256_refund_published,
        "sha256_l1_refund_command": (
            "TN10_SDK_DEV_PATCH=1 python examples/silverscript/htlc_sha256.py "
            "--refund-rehearsal --no-resume --publish-fixture"
        ),
        "l2_note": (
            "If L2 vault already claimed (happy path), timeout rehearsal is L1-only; "
            "deploy a fresh bridge for an unfunded-vault L2 timeout demo."
        ),
        "steps": pb["timeout_path"],
    }


def build_report(skip_gate: bool) -> dict[str, Any]:
    fixtures = [verify_fixture_offline(name) for name in FIXTURES]
    fixtures_ok = all(f.get("ok") for f in fixtures)
    adoption = adoption_card(skip_gate)
    gate_ok = bool(adoption.get("gateOk"))
    sdk = sdk_gate()
    return {
        "network": "testnet-10",
        "not_consensus": True,
        "gate": {
            "adoption_ok": gate_ok,
            "healthy": adoption.get("healthyCount"),
            "min_healthy": adoption.get("minHealthy", 2),
            "nodes": adoption.get("nodes"),
        },
        "sdk": sdk,
        "fixtures": {"all_ok": fixtures_ok, "items": fixtures},
        "galleon": {
            "wallet": galleon_wallet(),
            "bridges": bridge_status(),
            "pool": pool_status(),
        },
        "timeout_playbook": timeout_playbook(),
        "ready_for_cex_demo": gate_ok and fixtures_ok and bool(sdk.get("readyWithDevPatch")),
    }


def print_human(report: dict[str, Any]) -> None:
    gate = report["gate"]
    print("TN10 integrator status  (rehearsal only; not consensus)\n")
    print(
        f"adoption gate   {'green' if gate['adoption_ok'] else 'red'}  "
        f"({gate.get('healthy', '?')}/{gate.get('min_healthy', 2)} healthy)"
    )
    for node in gate.get("nodes") or []:
        if "error" in node:
            print(f"  {node.get('name', '?')}: unreachable")
        else:
            print(
                f"  {node.get('name', '?')}: {node.get('stage')} "
                f"daa={node.get('daa')} gap={node.get('gap')}"
            )
    sdk = report["sdk"]
    print(
        f"sdk gate        native={sdk.get('readyNative')}  "
        f"dev_patch={sdk.get('readyWithDevPatch')}"
    )
    fx = report["fixtures"]
    print(f"fixtures        {sum(1 for i in fx['items'] if i.get('ok'))}/{len(fx['items'])} offline ok")
    wal = report["galleon"]["wallet"]
    if wal.get("ok"):
        print(f"galleon primary {wal['address']}")
        print(f"  native {wal['native_ikas']:.6f} iKAS  wiKAS {wal['wikas']:.6f}")
    bridges = report["galleon"]["bridges"]
    if bridges.get("sha256"):
        s = bridges["sha256"]
        print(f"sha256 bridge   {s.get('bridge')}  claimed={s.get('claimed')}  vault={s.get('vault_balance')}")
    pool = report["galleon"]["pool"]
    if pool.get("ok"):
        r = pool["reserves"]
        print(f"mini pool       {pool['pool']}")
        print(f"  reserves {r}")
    tp = report["timeout_playbook"]
    print("\ntimeout / refund rehearsal:")
    print(f"  int L1 verify: {tp['int_l1_verify_refund']}")
    print(f"  sha256 L1 refund published: {tp['sha256_l1_refund_published']}")
    if not tp["sha256_l1_refund_published"]:
        print(f"  sha256 L1 refund cmd: {tp['sha256_l1_refund_command']}")
    print(f"\ncex demo ready  {report['ready_for_cex_demo']}")


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="TN10 + Galleon integrator status")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--skip-gate", action="store_true", help="skip cargo tn10-node-health")
    args = parser.parse_args()
    report = build_report(args.skip_gate)
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print_human(report)
    return 0 if report["fixtures"]["all_ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
