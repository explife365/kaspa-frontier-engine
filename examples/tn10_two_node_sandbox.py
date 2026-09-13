"""Two-node TN10 sandbox: N-of-M gate, test transfer, dual-node observation, proof bundle.

  python examples/tn10_two_node_sandbox.py --dry-run
  python examples/tn10_two_node_sandbox.py --transfer --kas 0.2
  python examples/tn10_two_node_sandbox.py --transfer --kas 0.2 --covenant
  python examples/tn10_two_node_sandbox.py --json

Requires 2/2 healthy owned nodes (18210 + 28210). Broadcast uses public TN10 wRPC;
owned nodes are polled via raw wRPC (kaspa-py RpcClient disconnects on loopback).
Not consensus evidence.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

import websockets

ROOT = Path(__file__).resolve().parents[1]
LOCAL = ROOT / ".local"
PROOF_PATH = LOCAL / "tn10-two-node-sandbox.json"
FIXTURE_COUNTER = ROOT / "fixtures" / "tn10-counter-proof.json"

sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "examples"))

from kaspa_env import load_kaspa_env  # noqa: E402
from tn10_ibd_watch import STAGE_HEALTHY, configured_nodes  # noqa: E402
from kaspa import kaspa_to_sompi  # noqa: E402
from tn10_rest import MIN_STORAGE_SAFE_SOMPI, virtual_daa  # noqa: E402
from tn10_wallets import get_wallet  # noqa: E402

NETWORK_ID = "testnet-10"
DEFAULT_NODES = ("ws://127.0.0.1:18210", "ws://127.0.0.1:28210")
NODE_OBSERVE_TIMEOUT_S = 90
NODE_OBSERVE_POLL_S = 2.0


def _run_json(cmd: list[str], timeout: int = 180) -> dict[str, Any]:
    proc = subprocess.run(
        cmd,
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )
    text = (proc.stdout or proc.stderr or "").strip()
    if not text:
        return {"ok": False, "exitCode": proc.returncode}
    try:
        body = json.loads(text)
    except json.JSONDecodeError:
        return {"ok": False, "raw": text[:500]}
    if isinstance(body, dict):
        body["exitCode"] = proc.returncode
    return body


def adoption_gate() -> dict[str, Any]:
    return _run_json(
        [sys.executable, str(ROOT / "scripts" / "tn10_adoption_scorecard.py"), "--json"],
        timeout=150,
    )


def node_urls() -> list[str]:
    return [url for _, url in configured_nodes()]


async def wrpc_call(url: str, method: str, params: dict[str, Any], req_id: int = 1) -> dict[str, Any]:
    async with websockets.connect(url, open_timeout=12, close_timeout=2, max_size=8 * 2**20) as ws:
        await ws.send(json.dumps({"id": req_id, "method": method, "params": params}))
        return json.loads(await asyncio.wait_for(ws.recv(), timeout=12))


async def probe_node_utxo(url: str, address: str, txid: str) -> dict[str, Any]:
    try:
        result = await wrpc_call(url, "getUtxosByAddresses", {"addresses": [address]})
        entries = (result.get("params") or {}).get("entries") or []
        saw = any(
            (entry.get("outpoint") or {}).get("transactionId") == txid for entry in entries
        )
        return {
            "url": url,
            "ok": True,
            "saw_utxo": saw,
            "utxo_count": len(entries),
        }
    except Exception as err:  # noqa: BLE001
        return {"url": url, "ok": False, "error": str(err), "saw_utxo": False}


async def observe_nodes_until(txid: str, dest_address: str) -> list[dict[str, Any]]:
    deadline = time.monotonic() + NODE_OBSERVE_TIMEOUT_S
    last: list[dict[str, Any]] = []
    while time.monotonic() < deadline:
        observations = [
            await probe_node_utxo(url, dest_address, txid) for url in node_urls()
        ]
        last = observations
        if all(o.get("ok") and o.get("saw_utxo") for o in observations):
            return observations
        await asyncio.sleep(NODE_OBSERVE_POLL_S)
    return last


def verify_counter_offline() -> dict[str, Any]:
    if not FIXTURE_COUNTER.is_file():
        return {"ok": False, "error": "missing counter fixture"}
    proc = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "--release",
            "--bin",
            "tn10-proof",
            "--",
            str(FIXTURE_COUNTER.relative_to(ROOT)),
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
        return {"ok": False, "error": (proc.stderr or text)[:300]}
    try:
        body = json.loads(text)
    except json.JSONDecodeError:
        return {"ok": False, "error": "json parse failed"}
    return {"ok": True, "covenant_id": body.get("covenant_id"), "fixture": str(FIXTURE_COUNTER)}


async def send_test_transfer(src: str, dest: str, kas: float) -> dict[str, Any]:
    # kaspa-py RpcClient drops loopback owned-node sockets; public resolver is stable.
    os.environ.pop("KASPA_RPC_URL", None)
    from tn10_transfer import send_kas  # noqa: E402

    txid = await send_kas(src, dest, kas, conf=1)
    wallet = get_wallet(dest, ROOT)
    observations = await observe_nodes_until(txid, wallet.address)
    return {
        "txid": txid,
        "from": src,
        "to": dest,
        "kas": kas,
        "broadcast_via": "public_resolver",
        "observe_nodes": node_urls(),
        "dest_address": wallet.address,
        "node_observations": observations,
        "both_nodes_saw_utxo": all(o.get("saw_utxo") for o in observations if o.get("ok")),
    }


def run_covenant_counter() -> dict[str, Any]:
    env = os.environ.copy()
    env["TN10_SDK_DEV_PATCH"] = "1"
    env.setdefault("TN10_OWNED_NODE_URLS", ",".join(DEFAULT_NODES))
    proc = subprocess.run(
        [sys.executable, str(ROOT / "examples" / "silverscript" / "counter.py")],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
        timeout=600,
        check=False,
    )
    return {
        "ok": proc.returncode == 0,
        "exitCode": proc.returncode,
        "stdout_tail": (proc.stdout or "")[-800:],
        "stderr_tail": (proc.stderr or "")[-400:] if proc.stderr else "",
        "local_proof": str(LOCAL / "tn10-covenant-proof.json"),
    }


def build_report(
    *,
    gate: dict[str, Any],
    transfer: dict[str, Any] | None,
    covenant: dict[str, Any] | None,
    offline: dict[str, Any],
) -> dict[str, Any]:
    nodes = gate.get("nodes") or []
    healthy = [n for n in nodes if n.get("stage") == STAGE_HEALTHY]
    return {
        "network": "testnet-10",
        "not_consensus": True,
        "generated_at": int(time.time()),
        "virtual_daa": virtual_daa(),
        "gate": {
            "ok": bool(gate.get("gateOk")),
            "healthy": len(healthy),
            "required": gate.get("minHealthy", 2),
            "nodes": nodes,
        },
        "transfer": transfer,
        "covenant_run": covenant,
        "offline_verify": offline,
        "proof_ok": bool(
            gate.get("gateOk")
            and (transfer is None or transfer.get("both_nodes_saw_utxo"))
            and offline.get("ok")
        ),
    }


def write_proof(report: dict[str, Any]) -> Path:
    LOCAL.mkdir(parents=True, exist_ok=True)
    PROOF_PATH.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return PROOF_PATH


async def run_sandbox(
    *,
    dry_run: bool,
    do_transfer: bool,
    kas: float,
    src: str,
    dest: str,
    covenant: bool,
) -> dict[str, Any]:
    load_kaspa_env(ROOT)
    os.environ.setdefault("TN10_OWNED_NODE_URLS", ",".join(DEFAULT_NODES))
    os.environ.setdefault("TN10_MIN_HEALTHY", "2")

    gate = adoption_gate()
    if not gate.get("gateOk"):
        report = build_report(gate=gate, transfer=None, covenant=None, offline={"ok": False, "skipped": True})
        write_proof(report)
        return report

    transfer_body: dict[str, Any] | None = None
    covenant_body: dict[str, Any] | None = None

    if do_transfer and not dry_run:
        if not node_urls():
            raise RuntimeError("no owned node URLs configured")
        transfer_body = await send_test_transfer(src, dest, kas)

    if covenant and not dry_run:
        covenant_body = run_covenant_counter()

    offline = verify_counter_offline()
    report = build_report(gate=gate, transfer=transfer_body, covenant=covenant_body, offline=offline)
    path = write_proof(report)
    report["proof_path"] = str(path)
    return report


def print_human(report: dict[str, Any]) -> None:
    print("TN10 two-node sandbox  (rehearsal; not consensus)\n")
    gate = report["gate"]
    print(f"gate           {'green' if gate['ok'] else 'red'}  {gate['healthy']}/{gate['required']} healthy")
    for node in gate.get("nodes") or []:
        print(f"  {node.get('name', '?')}: {node.get('stage')} daa={node.get('daa')} gap={node.get('gap')}")
    if report.get("transfer"):
        t = report["transfer"]
        print(f"\ntransfer       {t['from']} -> {t['to']}  {t['kas']} tKAS")
        print(f"txid           {t['txid']}")
        print(f"both nodes     {'yes' if t.get('both_nodes_saw_utxo') else 'no'}")
        for obs in t.get("node_observations") or []:
            print(f"  {obs['url']}  saw_utxo={obs.get('saw_utxo')} ok={obs.get('ok')}")
    off = report.get("offline_verify") or {}
    print(f"\noffline proof  {'ok' if off.get('ok') else 'fail'}  {off.get('fixture', '')}")
    print(f"bundle ok      {report.get('proof_ok')}")
    if report.get("proof_path"):
        print(f"proof file     {report['proof_path']}")


def main() -> int:
    parser = argparse.ArgumentParser(description="TN10 two-node transfer sandbox + proof")
    parser.add_argument("--dry-run", action="store_true", help="gate + offline verify only")
    parser.add_argument(
        "--transfer",
        action="store_true",
        help="broadcast test tx (public TN10 wRPC) and confirm on both owned nodes",
    )
    min_kas = MIN_STORAGE_SAFE_SOMPI / 1e8
    parser.add_argument(
        "--kas",
        type=float,
        default=min_kas,
        help=f"transfer amount tKAS (min ~{min_kas} for KIP-0009 storage mass)",
    )
    parser.add_argument("--from", dest="src", default="alice")
    parser.add_argument("--to", dest="dest", default="bob")
    parser.add_argument("--covenant", action="store_true", help="also run counter.py (slow; needs funded alice)")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    do_transfer = bool(args.transfer) and not args.dry_run
    if do_transfer and kaspa_to_sompi(args.kas) < MIN_STORAGE_SAFE_SOMPI:
        parser.error(
            f"--kas {args.kas} is below KIP-0009 floor "
            f"({MIN_STORAGE_SAFE_SOMPI} sompi / ~{min_kas} tKAS)"
        )

    report = asyncio.run(
        run_sandbox(
            dry_run=args.dry_run,
            do_transfer=do_transfer,
            kas=args.kas,
            src=args.src,
            dest=args.dest,
            covenant=args.covenant,
        )
    )
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print_human(report)
    return 0 if report.get("proof_ok") or (args.dry_run and report["gate"]["ok"]) else 1


if __name__ == "__main__":
    raise SystemExit(main())
