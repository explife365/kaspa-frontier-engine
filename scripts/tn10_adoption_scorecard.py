#!/usr/bin/env python3
"""TN10 owned-node adoption scorecard (IBD stages + optional N-of-M gate).

Not consensus evidence. Helps operators see what blocks deposits, wRPC, and covenant RPC.

  python scripts/tn10_adoption_scorecard.py
  python scripts/tn10_adoption_scorecard.py --json
  python scripts/tn10_adoption_scorecard.py --min-healthy 1
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from tn10_ibd_watch import (  # noqa: E402
    DEFAULT_LOG,
    STAGE_HEALTHY,
    collect_nodes,
    configured_nodes,
    utxoindex_resync_hint,
)
from tn10_rest import REST_BASE, get_json  # noqa: E402


def run_gate(min_healthy: int) -> dict | None:
    env = os.environ.copy()
    env.setdefault("TN10_MIN_HEALTHY", str(min_healthy))
    env.setdefault(
        "TN10_OWNED_NODE_URLS",
        "ws://127.0.0.1:18210,ws://127.0.0.1:28210",
    )
    cmd = [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "--bin",
        "tn10-node-health",
        "--",
        "--dual",
        "--min-healthy",
        str(min_healthy),
        "--json",
    ]
    try:
        proc = subprocess.run(
            cmd,
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
    except (subprocess.TimeoutExpired, OSError) as err:
        return {"error": str(err), "exitCode": -1}
    text = proc.stdout.strip() or proc.stderr.strip()
    if not text:
        return {"error": "empty gate output", "exitCode": proc.returncode}
    try:
        body = json.loads(text)
    except json.JSONDecodeError:
        return {"error": "gate json parse failed", "raw": text[:500], "exitCode": proc.returncode}
    body["exitCode"] = proc.returncode
    return body


def recommendations(nodes: list[dict], gate: dict | None, min_healthy: int) -> list[str]:
    recs: list[str] = []
    if not nodes:
        recs.append("Configure TN10_OWNED_NODE_URLS or use default 18210 + 28210 loopback wRPC.")
        return recs

    unreachable = sum(1 for n in nodes if n.get("stage") == "unreachable" or "error" in n)
    if unreachable == len(nodes):
        recs.append("Start owned TN10 kaspad: powershell -File scripts/tn10_node_onboard.ps1 -StartNode")
        recs.append(
            "Flags: --testnet --netsuffix=10 --utxoindex --disable-upnp "
            "--rpclisten=127.0.0.1:16210 --rpclisten-json=127.0.0.1:18210 "
            "--appdir %LOCALAPPDATA%\\kaspa\\tn10"
        )
        return recs

    for node in nodes:
        name = node.get("name", "?")
        stage = node.get("stage", "?")
        if stage == "missing_utxoindex":
            recs.append(f"{name}: restart with --utxoindex (required for deposit/covenant rehearsal).")
        elif stage == "utxo_commit":
            recs.append(f"{name}: UTXO index importing (DAA 0) — wait; do not credit deposits yet.")
        elif stage == "body_sync":
            recs.append(f"{name}: header/body gap high — let IBD finish before production ingestion.")
        elif stage == "ibd_peers":
            recs.append(f"{name}: still has IBD peers — wait for sync to complete.")
        elif stage == "no_peers":
            recs.append(f"{name}: no peers — check firewall / P2P 16211 and upstream connectivity.")
        elif stage == "finishing_sync":
            recs.append(f"{name}: connected but not synced — wait for isSynced=true.")
        elif stage == "dag_incomplete":
            recs.append(f"{name}: headers missing with DAA>0 — check appdir / corrupt IBD.")

    healthy = [n for n in nodes if n.get("stage") == STAGE_HEALTHY]
    if len(healthy) >= 1:
        recs.append("Probe deposits: cargo run --release --bin tn10-deposits -- <kaspatest:addr>")
        recs.append("Covenant shim: cargo run --release --bin tn10-covenant-rpc (loopback only)")
    if len(healthy) >= min_healthy:
        recs.append(f"N-of-M gate target met ({len(healthy)}/{min_healthy} healthy).")
        recs.append("wRPC rehearsal: cargo run --release --bin tn10-wrpc-live -- <addr> --dual --resnapshot-only")
    elif gate and gate.get("exitCode", 1) != 0:
        recs.append(
            f"Gate red: need {min_healthy} healthy owned nodes "
            "(second node: scripts/tn10_host02_tunnel.ps1 on remote host)."
        )

    recs.append("Full playbook: powershell -File scripts/tn10_node_onboard.ps1")
    return recs


def probe_public_rest() -> dict:
    """Read-only TN10 REST probe (no owned node). Yellow checklist for REST-only devs."""
    try:
        network = get_json("/info/network")
        blockdag = get_json("/info/blockdag")
    except (RuntimeError, OSError, ValueError, TypeError) as err:
        return {
            "name": "public-rest",
            "url": REST_BASE,
            "stage": "unreachable",
            "error": str(err),
        }
    try:
        daa = int(blockdag.get("virtualDaaScore") or 0)
        blocks = int(blockdag.get("blockCount") or 0)
        headers = int(blockdag.get("headerCount") or 0)
    except (TypeError, ValueError) as err:
        return {
            "name": "public-rest",
            "url": REST_BASE,
            "stage": "parse_error",
            "error": str(err),
        }
    return {
        "name": "public-rest",
        "url": REST_BASE,
        "stage": "reachable",
        "network": network.get("networkName") or network.get("network") or "testnet-10",
        "daa": daa,
        "blocks": blocks,
        "headers": headers,
        "gap": headers - blocks,
    }


def public_recommendations(public: dict) -> list[str]:
    recs: list[str] = []
    stage = public.get("stage")
    if stage != "reachable":
        recs.append(f"Public TN10 REST unreachable ({public.get('url', REST_BASE)}).")
        if public.get("error"):
            recs.append(f"Error: {public['error']}")
        recs.append("Retry later or use offline fixtures: cargo run --release --bin tn10-proof -- fixtures/tn10-counter-proof.json --offline")
        return recs

    recs.append("Public REST OK for read-only dev (tx lookup, blockdag, address paths).")
    recs.append("Do not credit deposits without owned synced nodes (--utxoindex).")
    recs.append(
        "Offline covenant verify: cargo run --release --bin tn10-proof -- fixtures/tn10-counter-proof.json --offline"
    )
    recs.append("Fixture pack: python examples/integrator_status.py --skip-gate (8 proofs, no IBD).")
    recs.append("Owned-node path: powershell -File scripts/tn10_node_onboard.ps1")
    recs.append("Full N-of-M gate: python scripts/tn10_adoption_scorecard.py (omit --public-only).")
    recs.append("Covenant UTXO index interim: kascov + rusty-kaspa#1128 getUtxosByCovenantId.")
    return recs


async def build_public_scorecard() -> dict:
    public = probe_public_rest()
    public_ok = public.get("stage") == "reachable"
    return {
        "mode": "public-only",
        "network": "testnet-10",
        "minHealthy": 0,
        "healthyCount": 0,
        "gateOk": False,
        "publicRestOk": public_ok,
        "utxoindexResync": None,
        "ready": {
            "publicRestOnly": public_ok,
            "ownedNodeProbe": False,
            "nOfMIngestion": False,
            "covenantRpcShim": False,
        },
        "publicRest": public,
        "nodes": [],
        "gate": None,
        "recommendations": public_recommendations(public),
    }


async def build_scorecard(min_healthy: int, skip_gate: bool) -> dict:
    log_path = Path(os.environ.get("TN10_KASPAD_LOG", str(DEFAULT_LOG)))
    nodes = await collect_nodes(configured_nodes())
    gate = None if skip_gate else run_gate(min_healthy)
    healthy_count = sum(1 for n in nodes if n.get("stage") == STAGE_HEALTHY)
    gate_ok = bool(gate and gate.get("exitCode") == 0)
    return {
        "mode": "owned-nodes",
        "network": "testnet-10",
        "minHealthy": min_healthy,
        "healthyCount": healthy_count,
        "gateOk": gate_ok,
        "utxoindexResync": utxoindex_resync_hint(log_path),
        "ready": {
            "publicRestOnly": True,
            "ownedNodeProbe": healthy_count >= 1,
            "nOfMIngestion": gate_ok,
            "covenantRpcShim": healthy_count >= 1,
        },
        "nodes": nodes,
        "gate": gate,
        "recommendations": recommendations(nodes, gate, min_healthy),
    }


def print_human(card: dict) -> None:
    mode = card.get("mode", "owned-nodes")
    print(f"TN10 adoption scorecard  mode={mode}")
    if mode == "public-only":
        public = card.get("publicRest") or {}
        print(f"public REST            {'green' if card.get('publicRestOk') else 'red'} ({public.get('url', REST_BASE)})")
        if public.get("stage") == "reachable":
            print(
                f"  daa={public.get('daa')} blocks={public.get('blocks')} "
                f"headers={public.get('headers')} gap={public.get('gap')}"
            )
        elif public.get("error"):
            print(f"  error: {public['error']}")
        print("owned-node gate        n/a (use without --public-only)")
    else:
        print(f"healthy nodes          {card['healthyCount']}/{card['minHealthy']}")
        print(f"gate                   {'green' if card['gateOk'] else 'red'}")
    if card.get("utxoindexResync"):
        print(f"utxoindex resync       {card['utxoindexResync']}")
    print()
    for node in card["nodes"]:
        if "error" in node:
            print(f"  {node.get('name','?')}: unreachable ({node['error']})")
            continue
        print(
            f"  {node['name']}: stage={node['stage']} daa={node['daa']} "
            f"gap={node['gap']} synced={node['synced']}"
        )
    print()
    print("ready:")
    for key, val in card["ready"].items():
        print(f"  {key}: {val}")
    print()
    print("next:")
    for line in card["recommendations"]:
        print(f"  - {line}")


async def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="TN10 node adoption scorecard")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--min-healthy", type=int, default=2)
    parser.add_argument("--skip-gate", action="store_true", help="IBD only (no cargo gate)")
    parser.add_argument(
        "--public-only",
        action="store_true",
        help="REST-only checklist (no owned-node wRPC probe or N-of-M gate)",
    )
    args = parser.parse_args(argv)

    if args.public_only:
        card = await build_public_scorecard()
        if args.json:
            print(json.dumps(card, indent=2))
        else:
            print_human(card)
        return 0 if card.get("publicRestOk") else 1

    card = await build_scorecard(args.min_healthy, args.skip_gate)
    if args.json:
        print(json.dumps(card, indent=2))
    else:
        print_human(card)
    return 0 if card["gateOk"] or args.skip_gate else 1


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
