"""L1 TN10 HTLC ↔ L2 Galleon wiKAS bridge playbook and operator commands.

Kaspa-native pattern: lock KAS on L1 with SilverScript HTLC; release wrapped iKAS on
Galleon when the buyer reveals the same int preimage demo tag. Timeout refunds on L1
if L2 never pays.

  python examples/l1_l2_htlc_bridge.py
  python examples/l1_l2_htlc_bridge.py --json

Operator flow (after deploy):
  python examples/galleon_entry.py --wallet dave --kas 1 --broadcast   # optional: tKAS → iKAS
  python examples/l1_l2_bridge_deploy.py --broadcast
  python examples/l1_l2_bridge_release.py --fund --broadcast
  python examples/l1_l2_bridge_release.py --claim --broadcast
  python examples/silverscript/htlc.py   # L1 claim with same preimage
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples" / "silverscript"))

from htlc import PAYMENT_HASH, PAYMENT_PREIMAGE_HASH  # noqa: E402
from htlc_sha256 import PAYMENT_HASH_HEX, PAYMENT_PREIMAGE  # noqa: E402

GALLEON_CHAIN_ID = 38836
WIKAS = "0x7331b0a33ac9aa92f506f057bfaa049ea133f77f"
GALLEON_MINI_POOL = "0xa423c4f6930e0bdb2fa32470441767dff3937d3d"
HTLC_CLAIM_FIXTURE = ROOT / "fixtures" / "tn10-htlc-proof.json"
HTLC_REFUND_FIXTURE = ROOT / "fixtures" / "tn10-htlc-refund-proof.json"
HTLC_SHA256_CLAIM_FIXTURE = ROOT / "fixtures" / "tn10-htlc-sha256-proof.json"
BRIDGE_DEPLOY = ROOT / "examples" / "l1_l2_bridge_deploy.py"
BRIDGE_RELEASE = ROOT / "examples" / "l1_l2_bridge_release.py"
BRIDGE_DEPLOY_SHA256 = ROOT / "examples" / "l1_l2_bridge_deploy_sha256.py"
BRIDGE_RELEASE_SHA256 = ROOT / "examples" / "l1_l2_bridge_release_sha256.py"


def bridge_address_from_env() -> str | None:
    addr = (os.environ.get("GALLEON_HTLC_BRIDGE") or "").strip()
    return addr or None


def bridge_sha256_address_from_env() -> str | None:
    addr = (os.environ.get("GALLEON_HTLC_BRIDGE_SHA256") or "").strip()
    return addr or None


def playbook() -> dict:
    bridge = bridge_address_from_env()
    return {
        "pattern": "l1_htlc_lock_l2_preimage_release",
        "networks": {
            "l1": "testnet-10",
            "l2": {"name": "Galleon", "chain_id": GALLEON_CHAIN_ID},
        },
        "l1": {
            "app": "examples/silverscript/htlc.py",
            "claim_fixture": str(HTLC_CLAIM_FIXTURE.relative_to(ROOT)),
            "refund_fixture": str(HTLC_REFUND_FIXTURE.relative_to(ROOT)),
            "demo_payment_hash": PAYMENT_HASH,
            "demo_preimage_int": PAYMENT_PREIMAGE_HASH,
            "verify_claim": "cargo run --release --bin tn10-proof -- fixtures/tn10-htlc-proof.json --offline",
            "verify_refund": "cargo run --release --bin tn10-proof -- fixtures/tn10-htlc-refund-proof.json --offline",
        },
        "ikas_gas": {
            "faucet": "python examples/galleon_faucet.py --drip-extra N && --sweep",
            "entry": "python examples/galleon_entry.py --wallet dave --kas 1 --broadcast",
            "deploy_preflight_ikas": 1.12,
        },
        "l2": {
            "wikas": WIKAS,
            "htlc_bridge_release": bridge,
            "deploy": f"python {BRIDGE_DEPLOY.relative_to(ROOT)} --broadcast",
            "fund": f"python {BRIDGE_RELEASE.relative_to(ROOT)} --fund --broadcast",
            "claim": f"python {BRIDGE_RELEASE.relative_to(ROOT)} --claim --broadcast",
            "status": f"python {BRIDGE_RELEASE.relative_to(ROOT)} --status",
            "galleon_mini_pool": GALLEON_MINI_POOL,
        },
        "happy_path": [
            "1. Deploy L2 vault: python examples/l1_l2_bridge_deploy.py --broadcast",
            "2. Fund wiKAS: python examples/l1_l2_bridge_release.py --fund --broadcast",
            "3. Seller locks KAS on L1: python examples/silverscript/htlc.py (genesis)",
            "4. Buyer claims L2: python examples/l1_l2_bridge_release.py --claim --broadcast",
            "5. Buyer claims L1 with same preimage: htlc.py claim step",
            "6. Verify: tn10-proof --offline + Galleon explorer txid",
        ],
        "timeout_path": [
            "1. python examples/l1_l2_bridge_timeout_playbook.py",
            "2. int: htlc.py --refund-rehearsal  |  sha256: htlc_sha256.py --refund-rehearsal",
            "3. KAS returns on L1 after refund_daa; L2 claim fails if vault unfunded or already claimed.",
        ],
        "integrator_status": "python examples/integrator_status.py --json",
        "sha256_variant": {
            "status": "l1_l2_claim_shipped",
            "l1_app": "examples/silverscript/htlc_sha256.py",
            "claim_fixture": str(HTLC_SHA256_CLAIM_FIXTURE.relative_to(ROOT)),
            "refund_fixture": "fixtures/tn10-htlc-sha256-refund-proof.json",
            "payment_hash_sha256": PAYMENT_HASH_HEX,
            "preimage": PAYMENT_PREIMAGE.decode("ascii"),
            "verify_claim": (
                "cargo run --release --bin tn10-proof -- "
                "fixtures/tn10-htlc-sha256-proof.json --offline"
            ),
            "verify_refund": (
                "cargo run --release --bin tn10-proof -- "
                "fixtures/tn10-htlc-sha256-refund-proof.json --offline"
            ),
            "l2_deploy": f"python {BRIDGE_DEPLOY_SHA256.relative_to(ROOT)} --broadcast",
            "l2_fund": f"python {BRIDGE_RELEASE_SHA256.relative_to(ROOT)} --fund --broadcast",
            "l2_claim": f"python {BRIDGE_RELEASE_SHA256.relative_to(ROOT)} --claim --broadcast",
            "l2_status": f"python {BRIDGE_RELEASE_SHA256.relative_to(ROOT)} --status",
            "htlc_bridge_release_sha256": bridge_sha256_address_from_env(),
            "refund_rehearsal": (
                "TN10_SDK_DEV_PATCH=1 python examples/silverscript/htlc_sha256.py "
                "--refund-rehearsal --no-resume --publish-fixture"
            ),
        },
        "not_shipped": [
            "Light-client proof of L1 inclusion on L2",
            "SilverScript bytes preimage in sig scripts (SHA256 demo uses int octets)",
        ],
    }


def main() -> None:
    sys.path.insert(0, str(ROOT / "scripts"))
    from kaspa_env import load_kaspa_env

    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="L1↔L2 HTLC bridge integrator playbook")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    data = playbook()
    if args.json:
        print(json.dumps(data, indent=2))
        return
    print("Kaspa L1 <-> Galleon L2 HTLC bridge\n")
    for step in data["happy_path"]:
        print(step)
    print("\nTimeout:")
    for step in data["timeout_path"]:
        print(step)
    print(f"\nL2 wiKAS: {WIKAS}")
    if data["l2"]["htlc_bridge_release"]:
        print(f"L2 bridge: {data['l2']['htlc_bridge_release']}")
    else:
        print("L2 bridge: not deployed (set GALLEON_HTLC_BRIDGE after deploy)")
    print(f"\nL1 verify claim:  {data['l1']['verify_claim']}")
    print(f"L1 verify refund: {data['l1']['verify_refund']}")
    sha = data["sha256_variant"]
    print(f"\nSHA256 variant ({sha['status']}):")
    print(f"  L1: {sha['l1_app']}  fixture {sha['claim_fixture']}")
    print(f"  L2 deploy: {sha['l2_deploy']}")
    if sha.get("htlc_bridge_release_sha256"):
        print(f"  L2 bridge: {sha['htlc_bridge_release_sha256']}")
    else:
        print(f"  blocker: {sha['blocker']}")


if __name__ == "__main__":
    main()
