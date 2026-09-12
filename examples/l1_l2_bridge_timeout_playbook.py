"""L1 HTLC timeout / refund rehearsal commands (integrator playbook).

Happy-path claim is in examples/l1_l2_htlc_bridge.py. This script documents and
optionally verifies the refund leg without requiring a second L2 vault when the
SHA256 bridge was already claimed on Galleon.

  python examples/l1_l2_bridge_timeout_playbook.py
  python examples/l1_l2_bridge_timeout_playbook.py --json
  python examples/l1_l2_bridge_timeout_playbook.py --verify-offline
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from kaspa_env import load_kaspa_env  # noqa: E402

INT_REFUND_PROOF = ROOT / "fixtures" / "tn10-htlc-refund-proof.json"
SHA256_REFUND_PROOF = ROOT / "fixtures" / "tn10-htlc-sha256-refund-proof.json"


def playbook() -> dict:
    return {
        "pattern": "l1_htlc_timeout_refund",
        "not_shipped": [
            "On-chain proof that L1 was refunded before L2 pays (light client)",
            "Automatic L2 vault return when L1 refunds (operator must not claim L2 after L1 refund)",
        ],
        "int_tag": {
            "fixture": str(INT_REFUND_PROOF.relative_to(ROOT)),
            "published": INT_REFUND_PROOF.is_file(),
            "verify": (
                "cargo run --release --bin tn10-proof -- "
                "fixtures/tn10-htlc-refund-proof.json --offline"
            ),
            "broadcast_refund": (
                "TN10_SDK_DEV_PATCH=1 python examples/silverscript/htlc.py "
                "--refund-rehearsal --no-resume --publish-fixture"
            ),
        },
        "sha256": {
            "fixture": str(SHA256_REFUND_PROOF.relative_to(ROOT)),
            "published": SHA256_REFUND_PROOF.is_file(),
            "verify": (
                "cargo run --release --bin tn10-proof -- "
                "fixtures/tn10-htlc-sha256-refund-proof.json --offline"
            ),
            "broadcast_refund": (
                "TN10_SDK_DEV_PATCH=1 python examples/silverscript/htlc_sha256.py "
                "--refund-rehearsal --no-resume --publish-fixture"
            ),
        },
        "l2_sha256_after_happy_path": {
            "note": (
                "GALLEON_HTLC_BRIDGE_SHA256 vault is claimed after the shipped happy path. "
                "Timeout rehearsal is L1 refund fixture + offline verify; redeploy bridge "
                "for a fresh unfunded L2 timeout demo."
            ),
            "status_cmd": "python examples/l1_l2_bridge_release_sha256.py --status",
        },
        "operator_sequence": [
            "1. Fund TN10 wallet (faucet / transfer).",
            "2. Run int or sha256 --refund-rehearsal (short timelock; wait ~12s for refund_daa).",
            "3. Publish fixture to fixtures/ with --publish-fixture.",
            "4. Verify: cargo run --release --bin tn10-proof -- <fixture> --offline",
            "5. integrator_status.py should show sha256_l1_refund_published: true",
        ],
    }


def verify_offline(path: Path) -> bool:
    if not path.is_file():
        print(f"missing {path}")
        return False
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
        ],
        cwd=ROOT,
        check=False,
    )
    return proc.returncode == 0


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="L1 HTLC timeout / refund playbook")
    parser.add_argument("--json", action="store_true")
    parser.add_argument(
        "--verify-offline",
        action="store_true",
        help="run tn10-proof --offline on published refund fixtures",
    )
    args = parser.parse_args()
    data = playbook()
    if args.verify_offline:
        ok_int = verify_offline(INT_REFUND_PROOF)
        ok_sha = verify_offline(SHA256_REFUND_PROOF) if SHA256_REFUND_PROOF.is_file() else None
        data["verify_results"] = {
            "int_refund": ok_int,
            "sha256_refund": ok_sha,
        }
        if args.json:
            print(json.dumps(data, indent=2))
            return 0 if ok_int and (ok_sha is None or ok_sha) else 1
        print(f"int refund offline:    {'ok' if ok_int else 'FAIL'}")
        if ok_sha is None:
            print("sha256 refund offline: not published yet")
        else:
            print(f"sha256 refund offline: {'ok' if ok_sha else 'FAIL'}")
        return 0 if ok_int and (ok_sha is None or ok_sha) else 1
    if args.json:
        print(json.dumps(data, indent=2))
        return 0
    print("L1 HTLC timeout / refund rehearsal\n")
    for step in data["operator_sequence"]:
        print(step)
    print(f"\nint refund published:    {data['int_tag']['published']}")
    print(f"sha256 refund published: {data['sha256']['published']}")
    if not data["sha256"]["published"]:
        print(f"\nRun: {data['sha256']['broadcast_refund']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
