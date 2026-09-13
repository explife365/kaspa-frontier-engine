"""Temporary integrator workarounds with native swap-in when blockers clear.

Each shim tries the future native path first, then falls back to today's rehearsal
tooling. Call sites stay stable when kaspad RPC, SDK wheel, or hosted nodes ship.

  python scripts/integrator_shims.py --json
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))


@dataclass(frozen=True)
class BlockerShim:
    id: str
    blocker: str
    shim: str
    unblock: str
    swap_command: str


BLOCKERS: tuple[BlockerShim, ...] = (
    BlockerShim(
        id="covenant_utxo_index",
        blocker="getUtxosByCovenantId not on kaspad RPC",
        shim="kascov covenant docs + tn10-covenant-rpc (loopback)",
        unblock="rusty-kaspa#1128",
        swap_command="integrator_shims.covenant_utxos()",
    ),
    BlockerShim(
        id="covenant_sdk_broadcast",
        blocker="PyPI kaspa-python-sdk wheel lacks Toccata computeBudget",
        shim="TN10_SDK_DEV_PATCH=1 (rehearsal only)",
        unblock="kaspa-python-sdk#78 merged + wheel",
        swap_command="integrator_shims.sdk_broadcast_gate()",
    ),
    BlockerShim(
        id="owned_node_ingestion",
        blocker="Public REST cannot credit exchange deposits",
        shim="tn10_adoption_scorecard.py --public-only + offline fixtures",
        unblock="Owned kaspad --utxoindex + N-of-M gate (2/2)",
        swap_command="integrator_shims.adoption_gate()",
    ),
    BlockerShim(
        id="l1_evm_defi",
        blocker="kaspad has no EVM runtime",
        shim="Igra Galleon L2 (gTEST, wiKAS, FeePool, games)",
        unblock="Keep L2; L1 stays UTXO + Toccata",
        swap_command="integrator_shims.galleon_stack()",
    ),
    BlockerShim(
        id="public_wrpc",
        blocker="No public TN10 wRPC for custody",
        shim="Loopback ws://127.0.0.1:18210 + host02 tunnel 28210",
        unblock="Independently hosted nodes per integrator",
        swap_command="integrator_shims.owned_node_urls()",
    ),
)


def _run_json(cmd: list[str], timeout: int = 120) -> dict[str, Any]:
    try:
        proc = subprocess.run(
            cmd,
            cwd=ROOT,
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
        return {"ok": False, "raw": text[:400]}
    if isinstance(body, dict):
        body.setdefault("ok", proc.returncode == 0)
    return body


def sdk_broadcast_gate() -> dict[str, Any]:
    """Prefer native wheel; dev patch is interim."""
    env = os.environ.copy()
    native = _run_json([sys.executable, str(ROOT / "scripts" / "tn10_sdk_gate.py"), "--json"], timeout=60)
    if native.get("readyNative"):
        return {
            "source": "native",
            "ready": True,
            "shim_active": False,
            "detail": native,
            "swap_when": "kaspa-python-sdk#78 wheel on PyPI",
        }
    env.setdefault("TN10_SDK_DEV_PATCH", "1")
    patched = _run_json(
        [sys.executable, str(ROOT / "scripts" / "tn10_sdk_gate.py"), "--json"],
        timeout=60,
    )
    return {
        "source": "dev_patch",
        "ready": bool(patched.get("readyWithDevPatch")),
        "shim_active": True,
        "detail": patched,
        "swap_when": "unset TN10_SDK_DEV_PATCH after readyNative",
    }


def adoption_gate(public_only: bool = False) -> dict[str, Any]:
    """Prefer owned-node N-of-M; public REST is yellow checklist."""
    cmd = [sys.executable, str(ROOT / "scripts" / "tn10_adoption_scorecard.py"), "--json"]
    if public_only:
        cmd.append("--public-only")
    card = _run_json(cmd, timeout=150)
    if public_only:
        return {
            "source": "public_rest",
            "ready": bool(card.get("publicRestOk")),
            "shim_active": True,
            "detail": card,
            "swap_when": "omit --public-only when 2/2 owned nodes healthy",
        }
    return {
        "source": "owned_nodes",
        "ready": bool(card.get("gateOk")),
        "shim_active": not card.get("gateOk"),
        "detail": card,
        "swap_when": "powershell -File scripts/tn10_node_onboard.ps1",
    }


def covenant_utxos_route() -> dict[str, Any]:
    """Document routing: native RPC slot reserved; today kascov + shim."""
    native_url = (os.environ.get("TN10_KASPAD_RPC_URL") or "").strip()
    shim_url = (os.environ.get("TN10_COVENANT_RPC_URL") or "http://127.0.0.1:18330").strip()
    kascov_base = "https://kascov.io/data/testnet-10/c"
    return {
        "source": "kascov_shim",
        "ready": True,
        "shim_active": True,
        "native_rpc_configured": bool(native_url),
        "routes": {
            "native_when_live": "getUtxosByCovenantId on kaspad (rusty-kaspa#1128)",
            "today_indexer": f"{kascov_base}/<covenant_id>.json",
            "today_shim": f"{shim_url} (tn10-covenant-rpc)",
            "verify": "cargo run --release --bin tn10-proof -- fixtures/tn10-counter-proof.json --offline",
        },
        "swap_when": "native RPC returns verified rows; drop kascov-only label",
    }


def galleon_stack() -> dict[str, Any]:
    keys = ("GALLEON_MINI_POOL", "GALLEON_FEE_POOL", "GALLEON_COIN_FLIP", "GALLEON_BRIDGE_FACTORY")
    configured = {k: (os.environ.get(k) or "").strip() or None for k in keys}
    return {
        "source": "galleon_l2",
        "ready": any(configured.values()),
        "shim_active": True,
        "detail": configured,
        "swap_when": "L1 DeFi not planned; extend Galleon production when Igra ships mainnet tokens",
    }


def owned_node_urls() -> dict[str, Any]:
    raw = (os.environ.get("TN10_OWNED_NODE_URLS") or "ws://127.0.0.1:18210,ws://127.0.0.1:28210").strip()
    urls = [u.strip() for u in raw.split(",") if u.strip()]
    return {
        "source": "loopback_owned",
        "ready": len(urls) >= 2,
        "shim_active": True,
        "urls": urls,
        "swap_when": "replace with independently hosted wRPC endpoints per environment",
    }


def build_report() -> dict[str, Any]:
    routes: dict[str, Any] = {
        "covenant_utxo_index": covenant_utxos_route(),
        "covenant_sdk_broadcast": sdk_broadcast_gate(),
        "owned_node_ingestion": adoption_gate(public_only=False),
        "public_rest_fallback": adoption_gate(public_only=True),
        "l1_evm_defi": galleon_stack(),
        "public_wrpc": owned_node_urls(),
    }
    shims_active = sum(1 for v in routes.values() if v.get("shim_active"))
    return {
        "network": "testnet-10",
        "not_consensus": True,
        "blockers": [
            {
                "id": b.id,
                "blocker": b.blocker,
                "shim": b.shim,
                "unblock": b.unblock,
                "swap_command": b.swap_command,
                "route": routes.get(b.id) or routes.get(f"{b.id}_fallback"),
            }
            for b in BLOCKERS
        ],
        "routes": routes,
        "shims_active": shims_active,
        "ready_for_demo": bool(
            routes["covenant_sdk_broadcast"].get("ready")
            and routes["owned_node_ingestion"].get("ready")
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Integrator blocker shims")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    report = build_report()
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print("TN10 integrator shims (temporary until native paths ship)\n")
        for row in report["blockers"]:
            route = row.get("route") or {}
            status = "shim" if route.get("shim_active") else "native"
            print(f"[{status}] {row['id']}")
            print(f"  blocker  {row['blocker']}")
            print(f"  today    {row['shim']}")
            print(f"  unblock  {row['unblock']}\n")
        print(f"demo ready  {report['ready_for_demo']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
