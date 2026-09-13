"""Deploy GalleonFeePool on Galleon testnet (30 bps fee, treasury = deployer).

  python examples/galleon_fee_pool_deploy.py --simulate
  python examples/galleon_fee_pool_deploy.py --broadcast
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GALLEON_DIR = ROOT / "examples" / "galleon"
ARTIFACT = GALLEON_DIR / "out" / "GalleonFeePool.sol" / "GalleonFeePool.json"

sys.path.insert(0, str(ROOT / "scripts"))
from galleon import GALLEON_CHAIN_ID, GALLEON_EXPLORER, GALLEON_GTEST, GALLEON_MIN_GAS_WEI, GALLEON_RPC, GALLEON_WRAPPED_IKAS  # noqa: E402
from galleon_pool_deploy import galleon_key  # noqa: E402
from kaspa_env import load_kaspa_env, upsert_kaspa_env  # noqa: E402

FEE_BPS = 30
PROTOCOL_SHARE_BPS = 5000


def ensure_artifact() -> dict:
    if not ARTIFACT.is_file():
        subprocess.run(["forge", "build"], cwd=GALLEON_DIR, check=True)
    return json.loads(ARTIFACT.read_text(encoding="utf-8"))


def encode_fee_pool_args(token0: str, token1: str, treasury: str) -> str:
    t0 = token0.lower().removeprefix("0x").zfill(64)
    t1 = token1.lower().removeprefix("0x").zfill(64)
    tr = treasury.lower().removeprefix("0x").zfill(64)
    fb = f"{FEE_BPS:064x}"
    ps = f"{PROTOCOL_SHARE_BPS:064x}"
    return t0 + t1 + tr + fb + ps


def deploy_data(artifact: dict, token0: str, token1: str, treasury: str) -> str:
    bytecode = artifact["bytecode"]["object"].removeprefix("0x")
    return "0x" + bytecode + encode_fee_pool_args(token0, token1, treasury)


def forge_broadcast() -> str:
    key = galleon_key()
    cmd = [
        "forge",
        "script",
        "script/GalleonFeePool.s.sol:GalleonFeePoolScript",
        "--rpc-url",
        GALLEON_RPC,
        "--broadcast",
        "--private-key",
        key,
        "--legacy",
        "--with-gas-price",
        str(GALLEON_MIN_GAS_WEI),
    ]
    proc = subprocess.run(cmd, cwd=GALLEON_DIR, capture_output=True, text=True)
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr.strip() or proc.stdout.strip() or "forge broadcast failed")
    run_json = GALLEON_DIR / "broadcast" / "GalleonFeePool.s.sol" / str(GALLEON_CHAIN_ID) / "run-latest.json"
    body = json.loads(run_json.read_text(encoding="utf-8"))
    for tx in body.get("transactions", []):
        addr = (tx.get("contractAddress") or "").strip()
        if addr:
            return addr
    raise RuntimeError("no contractAddress in broadcast artifact")


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Deploy GalleonFeePool")
    parser.add_argument("--simulate", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    parser.add_argument("--token0", default=GALLEON_GTEST)
    parser.add_argument("--token1", default=GALLEON_WRAPPED_IKAS or "")
    args = parser.parse_args()

    from eth_account import Account

    treasury = Account.from_key(galleon_key()).address
    artifact = ensure_artifact()
    data = deploy_data(artifact, args.token0, args.token1, treasury)
    print(f"chain {GALLEON_CHAIN_ID}  treasury {treasury}")
    print(f"fee {FEE_BPS} bps  protocol share {PROTOCOL_SHARE_BPS} bps")

    if args.simulate or not args.broadcast:
        subprocess.run(
            ["forge", "script", "script/GalleonFeePool.s.sol:GalleonFeePoolScript", "--rpc-url", GALLEON_RPC],
            cwd=GALLEON_DIR,
            check=True,
        )
        print("simulate OK")
        return 0

    pool = forge_broadcast()
    print(f"pool  {pool}")
    print(f"      {GALLEON_EXPLORER}/address/{pool}")
    upsert_kaspa_env({"GALLEON_FEE_POOL": pool}, ROOT)
    print("saved GALLEON_FEE_POOL in kaspa.env")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
