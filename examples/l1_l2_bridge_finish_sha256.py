"""Deploy (if needed), fund, and claim the Galleon SHA256 HTLC bridge leg.

  python examples/l1_l2_bridge_finish_sha256.py --status
  python examples/l1_l2_bridge_finish_sha256.py --broadcast

Re-deploys when GALLEON_HTLC_BRIDGE_SHA256 payout != demo 0.001 wiKAS and wallet has gas.
"""

from __future__ import annotations

import argparse
import importlib.util
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from galleon import GALLEON_RPC  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402
from l1_l2_bridge_release_sha256 import (  # noqa: E402
    SELECTOR_PAYOUT,
    SELECTOR_VAULT_BALANCE,
    claim_vault,
    read_uint,
    status,
)

EXPECTED_PAYOUT = 10**15  # 0.001 wiKAS demo


def _load_deploy():
    path = ROOT / "examples" / "l1_l2_bridge_deploy_sha256.py"
    spec = importlib.util.spec_from_file_location("l1_l2_bridge_deploy_sha256", path)
    mod = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(mod)
    return mod


def bridge_from_env() -> str:
    addr = (os.environ.get("GALLEON_HTLC_BRIDGE_SHA256") or "").strip()
    return addr


def payout_ok(rpc: str, bridge: str) -> bool:
    if not bridge:
        return False
    return read_uint(rpc, bridge, SELECTOR_PAYOUT) == EXPECTED_PAYOUT


def needs_fund(rpc: str, bridge: str) -> bool:
    payout = read_uint(rpc, bridge, SELECTOR_PAYOUT)
    vault = read_uint(rpc, bridge, SELECTOR_VAULT_BALANCE)
    return vault < payout


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Finish Galleon SHA256 HTLC bridge leg")
    parser.add_argument("--status", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    parser.add_argument("--force-deploy", action="store_true", help="deploy even if env bridge exists")
    args = parser.parse_args()

    rpc = GALLEON_RPC
    bridge = bridge_from_env()

    if args.status or not args.broadcast:
        if bridge:
            body = status(rpc, bridge)
            body["payout_ok"] = body["payout_amount"] == EXPECTED_PAYOUT
            import json

            print(json.dumps(body, indent=2))
        else:
            print("no GALLEON_HTLC_BRIDGE_SHA256 in kaspa.env")
        return 0

    deploy_mod = _load_deploy()
    if args.force_deploy or not bridge or not payout_ok(rpc, bridge):
        if bridge and not payout_ok(rpc, bridge):
            on_chain = read_uint(rpc, bridge, SELECTOR_PAYOUT)
            print(
                f"bridge {bridge} payout {on_chain} != demo {EXPECTED_PAYOUT}; redeploying..."
            )
        artifact = deploy_mod.ensure_artifact()
        deploy_mod.preflight_galleon_balance(artifact)
        bridge = deploy_mod.raw_broadcast(artifact)
        from kaspa_env import upsert_kaspa_env

        upsert_kaspa_env({"GALLEON_HTLC_BRIDGE_SHA256": bridge}, ROOT)
        print(f"saved GALLEON_HTLC_BRIDGE_SHA256={bridge}")
        if not payout_ok(rpc, bridge):
            raise RuntimeError("deploy verification failed: payout mismatch")

    from l1_l2_bridge_release_sha256 import fund_vault
    from htlc_sha256 import PAYMENT_PREIMAGE  # noqa: E402

    payout = read_uint(rpc, bridge, SELECTOR_PAYOUT)
    if needs_fund(rpc, bridge):
        fund_vault(rpc, bridge, payout, broadcast=True)
    claim_vault(rpc, bridge, PAYMENT_PREIMAGE, broadcast=True)
    print("SHA256 L2 bridge leg complete")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
