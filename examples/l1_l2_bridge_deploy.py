"""Deploy HtlcBridgeRelease on Galleon (wiKAS vault for TN10 HTLC demo hashlock).

  python examples/l1_l2_bridge_deploy.py --simulate
  python examples/l1_l2_bridge_deploy.py --broadcast

Requires GALLEON_PRIVATE_KEY. Writes GALLEON_HTLC_BRIDGE to kaspa.env on broadcast.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GALLEON_DIR = ROOT / "examples" / "galleon"
ARTIFACT = GALLEON_DIR / "out" / "HtlcBridgeRelease.sol" / "HtlcBridgeRelease.json"

sys.path.insert(0, str(ROOT / "scripts"))
from galleon import (  # noqa: E402
    GALLEON_CHAIN_ID,
    GALLEON_EXPLORER,
    GALLEON_MIN_GAS_WEI,
    GALLEON_RPC,
    GALLEON_WRAPPED_IKAS,
    HTLC_BRIDGE_DEPLOY_IKAS,
)
from galleon_faucet import address_of, galleon_key, rpc_hex  # noqa: E402
from kaspa_env import load_kaspa_env, upsert_kaspa_env  # noqa: E402

# Matches examples/silverscript/htlc.py PAYMENT_HASH demo tag.
PAYMENT_HASH_INT = 0x48544C43
PAYOUT_WEI = int(0.001 * 1e18)
MIN_DEPLOY_IKAS_WEI = int(HTLC_BRIDGE_DEPLOY_IKAS * 1e18)
FORGE_BROADCAST_TIMEOUT_S = 180


def ensure_artifact() -> dict:
    if not ARTIFACT.is_file():
        subprocess.run(["forge", "build"], cwd=GALLEON_DIR, check=True)
    return json.loads(ARTIFACT.read_text(encoding="utf-8"))


def preflight_galleon_balance() -> None:
    key = galleon_key()
    addr = address_of(key)
    bal = int(rpc_hex("eth_getBalance", [addr, "latest"]), 16)
    print(f"wallet  {addr}  balance {bal / 1e18:.4f} iKAS")
    if bal < MIN_DEPLOY_IKAS_WEI:
        raise RuntimeError(
            f"need ~{MIN_DEPLOY_IKAS_WEI / 1e18:.1f} iKAS prepaid at {GALLEON_MIN_GAS_WEI} gwei "
            f"(have {bal / 1e18:.4f}). Run: python examples/galleon_faucet.py --drip"
        )


def forge_broadcast() -> str:
    key = galleon_key()
    if not key.startswith("0x"):
        key = f"0x{key}"
    print("broadcasting HtlcBridgeRelease (forge script)...")
    cmd = [
        "forge",
        "script",
        "script/HtlcBridgeRelease.s.sol:HtlcBridgeReleaseScript",
        "--rpc-url",
        GALLEON_RPC,
        "--broadcast",
        "--private-key",
        key,
        "--legacy",
        "--with-gas-price",
        str(GALLEON_MIN_GAS_WEI),
    ]
    proc = subprocess.run(
        cmd,
        cwd=GALLEON_DIR,
        capture_output=True,
        text=True,
        timeout=FORGE_BROADCAST_TIMEOUT_S,
    )
    if proc.returncode != 0:
        detail = proc.stderr.strip() or proc.stdout.strip() or "forge broadcast failed"
        raise RuntimeError(detail)
    if proc.stdout.strip():
        print(proc.stdout.strip())
    run_json = (
        GALLEON_DIR
        / "broadcast"
        / "HtlcBridgeRelease.s.sol"
        / str(GALLEON_CHAIN_ID)
        / "run-latest.json"
    )
    if not run_json.is_file():
        raise RuntimeError(f"missing broadcast artifact {run_json}")
    body = json.loads(run_json.read_text(encoding="utf-8"))
    for tx in body.get("transactions", []):
        addr = (tx.get("contractAddress") or "").strip()
        if addr:
            return addr
    raise RuntimeError("broadcast succeeded but no contractAddress in run-latest.json")


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Deploy HtlcBridgeRelease on Galleon")
    parser.add_argument("--simulate", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    args = parser.parse_args()

    ensure_artifact()
    print(f"chain   {GALLEON_CHAIN_ID}  rpc {GALLEON_RPC}", flush=True)
    print(f"wikas   {GALLEON_WRAPPED_IKAS}")
    print(f"hash    {PAYMENT_HASH_INT} (0x{PAYMENT_HASH_INT:08x})")
    print(f"payout  {PAYOUT_WEI} wei wiKAS")

    if args.simulate or not args.broadcast:
        subprocess.run(
            [
                "forge",
                "script",
                "script/HtlcBridgeRelease.s.sol:HtlcBridgeReleaseScript",
                "--rpc-url",
                GALLEON_RPC,
            ],
            cwd=GALLEON_DIR,
            check=True,
        )
        print("simulate OK — use --broadcast when wallet has prepaid iKAS at 2000 gwei")
        return 0

    preflight_galleon_balance()
    bridge = forge_broadcast()
    print(f"bridge  {bridge}")
    print(f"        {GALLEON_EXPLORER}/address/{bridge}")
    upsert_kaspa_env({"GALLEON_HTLC_BRIDGE": bridge}, ROOT)
    print("saved GALLEON_HTLC_BRIDGE in kaspa.env")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
