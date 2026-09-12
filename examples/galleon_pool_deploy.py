"""Deploy GalleonMiniPool on Galleon testnet (gTEST / wiKAS pair).

  python examples/galleon_pool_deploy.py --simulate
  python examples/galleon_pool_deploy.py --broadcast

Requires GALLEON_PRIVATE_KEY in kaspa.env. Writes GALLEON_MINI_POOL on success.
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
ARTIFACT = GALLEON_DIR / "out" / "GalleonMiniPool.sol" / "GalleonMiniPool.json"

sys.path.insert(0, str(ROOT / "scripts"))
from galleon import (  # noqa: E402
    GALLEON_CHAIN_ID,
    GALLEON_EXPLORER,
    GALLEON_GTEST,
    GALLEON_MIN_GAS_WEI,
    GALLEON_RPC,
    GALLEON_WRAPPED_IKAS,
)
from kaspa_env import load_kaspa_env, upsert_kaspa_env  # noqa: E402


def ensure_artifact() -> dict:
    if not ARTIFACT.is_file():
        subprocess.run(["forge", "build"], cwd=GALLEON_DIR, check=True)
    return json.loads(ARTIFACT.read_text(encoding="utf-8"))


def encode_constructor_args(token0: str, token1: str) -> str:
    t0 = token0.lower().removeprefix("0x").zfill(64)
    t1 = token1.lower().removeprefix("0x").zfill(64)
    return t0 + t1


def deploy_data(artifact: dict, token0: str, token1: str) -> str:
    bytecode = artifact["bytecode"]["object"]
    if bytecode.startswith("0x"):
        bytecode = bytecode[2:]
    return "0x" + bytecode + encode_constructor_args(token0, token1)


def galleon_key() -> str:
    key = (os.environ.get("GALLEON_PRIVATE_KEY") or "").strip()
    if not key:
        raise RuntimeError("missing GALLEON_PRIVATE_KEY; run examples/galleon_faucet.py --ensure-wallet")
    return key if key.startswith("0x") else f"0x{key}"


def forge_broadcast() -> str:
    key = galleon_key()
    cmd = [
        "forge",
        "script",
        "script/GalleonMiniPool.s.sol:GalleonMiniPoolScript",
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
    run_json = GALLEON_DIR / "broadcast" / "GalleonMiniPool.s.sol" / str(GALLEON_CHAIN_ID) / "run-latest.json"
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
    parser = argparse.ArgumentParser(description="Deploy GalleonMiniPool on Galleon testnet")
    parser.add_argument("--simulate", action="store_true", help="forge dry-run only")
    parser.add_argument("--broadcast", action="store_true", help="deploy and save GALLEON_MINI_POOL")
    parser.add_argument("--token0", default=GALLEON_GTEST)
    parser.add_argument("--token1", default=GALLEON_WRAPPED_IKAS or "")
    args = parser.parse_args()

    artifact = ensure_artifact()
    data = deploy_data(artifact, args.token0, args.token1)
    print(f"chain  {GALLEON_CHAIN_ID}  rpc {GALLEON_RPC}")
    print(f"pair   {args.token0} / {args.token1}")
    print(f"deploy calldata bytes {len(data) // 2 - 1}")

    if args.simulate or not args.broadcast:
        cmd = [
            "forge",
            "script",
            "script/GalleonMiniPool.s.sol:GalleonMiniPoolScript",
            "--rpc-url",
            GALLEON_RPC,
        ]
        subprocess.run(cmd, cwd=GALLEON_DIR, check=True)
        print("simulate OK — use --broadcast when wallet has ~1.71 iKAS prepaid at 2000 gwei")
        return 0

    pool = forge_broadcast()
    print(f"pool   {pool}")
    print(f"       {GALLEON_EXPLORER}/address/{pool}")
    upsert_kaspa_env({"GALLEON_MINI_POOL": pool}, ROOT)
    print("saved GALLEON_MINI_POOL in kaspa.env")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
