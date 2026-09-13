"""Deploy HtlcBridgeFactory on Galleon testnet (1% claim fee to deployer treasury).

  python examples/galleon_bridge_factory_deploy.py --simulate
  python examples/galleon_bridge_factory_deploy.py --broadcast
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GALLEON_DIR = ROOT / "examples" / "galleon"
ARTIFACT = GALLEON_DIR / "out" / "HtlcBridgeFactory.sol" / "HtlcBridgeFactory.json"

sys.path.insert(0, str(ROOT / "scripts"))
from galleon import GALLEON_CHAIN_ID, GALLEON_EXPLORER, GALLEON_MIN_GAS_WEI, GALLEON_RPC, GALLEON_WRAPPED_IKAS  # noqa: E402
from galleon_pool_deploy import galleon_key  # noqa: E402
from kaspa_env import load_kaspa_env, upsert_kaspa_env  # noqa: E402

CLAIM_FEE_BPS = 100


def forge_broadcast() -> str:
    key = galleon_key()
    cmd = [
        "forge",
        "script",
        "script/HtlcBridgeFactory.s.sol:HtlcBridgeFactoryScript",
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
    run_json = GALLEON_DIR / "broadcast" / "HtlcBridgeFactory.s.sol" / str(GALLEON_CHAIN_ID) / "run-latest.json"
    body = json.loads(run_json.read_text(encoding="utf-8"))
    for tx in body.get("transactions", []):
        addr = (tx.get("contractAddress") or "").strip()
        if addr:
            return addr
    raise RuntimeError("no contractAddress in broadcast artifact")


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Deploy HtlcBridgeFactory")
    parser.add_argument("--simulate", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    args = parser.parse_args()

    if not GALLEON_WRAPPED_IKAS:
        raise SystemExit("missing GALLEON_WRAPPED_IKAS")

    if args.simulate or not args.broadcast:
        subprocess.run(
            ["forge", "script", "script/HtlcBridgeFactory.s.sol:HtlcBridgeFactoryScript", "--rpc-url", GALLEON_RPC],
            cwd=GALLEON_DIR,
            check=True,
        )
        print(f"simulate OK  claim fee {CLAIM_FEE_BPS} bps")
        return 0

    factory = forge_broadcast()
    print(f"factory {factory}")
    print(f"        {GALLEON_EXPLORER}/address/{factory}")
    upsert_kaspa_env({"GALLEON_BRIDGE_FACTORY": factory}, ROOT)
    print("saved GALLEON_BRIDGE_FACTORY in kaspa.env")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
