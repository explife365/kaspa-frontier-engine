"""Deploy GalleonCoinFlip, GalleonDice, and GalleonJackpot on Galleon testnet.

  python examples/galleon_games_deploy.py --simulate
  python examples/galleon_games_deploy.py --broadcast
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GALLEON_DIR = ROOT / "examples" / "galleon"

sys.path.insert(0, str(ROOT / "scripts"))
from galleon import GALLEON_CHAIN_ID, GALLEON_EXPLORER, GALLEON_MIN_GAS_WEI, GALLEON_RPC  # noqa: E402
from galleon_pool_deploy import galleon_key  # noqa: E402
from kaspa_env import load_kaspa_env, upsert_kaspa_env  # noqa: E402

CONTRACT_KEYS = (
    ("GalleonCoinFlip", "GALLEON_COIN_FLIP"),
    ("GalleonDice", "GALLEON_DICE"),
    ("GalleonJackpot", "GALLEON_JACKPOT"),
)


def forge_broadcast() -> dict[str, str]:
    key = galleon_key()
    cmd = [
        "forge",
        "script",
        "script/GalleonGames.s.sol:GalleonGamesScript",
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
    run_json = GALLEON_DIR / "broadcast" / "GalleonGames.s.sol" / str(GALLEON_CHAIN_ID) / "run-latest.json"
    body = json.loads(run_json.read_text(encoding="utf-8"))
    found: dict[str, str] = {}
    for tx in body.get("transactions", []):
        name = (tx.get("contractName") or "").strip()
        addr = (tx.get("contractAddress") or "").strip()
        if name and addr:
            found[name] = addr
    return found


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Deploy Galleon DeFi games")
    parser.add_argument("--simulate", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    args = parser.parse_args()

    if args.simulate or not args.broadcast:
        subprocess.run(
            ["forge", "script", "script/GalleonGames.s.sol:GalleonGamesScript", "--rpc-url", GALLEON_RPC],
            cwd=GALLEON_DIR,
            check=True,
        )
        print("simulate OK — coin flip, dice, jackpot")
        return 0

    found = forge_broadcast()
    env_updates: dict[str, str] = {}
    for contract, env_key in CONTRACT_KEYS:
        addr = found.get(contract)
        if not addr:
            raise RuntimeError(f"missing deploy address for {contract}")
        print(f"{contract:16} {addr}")
        print(f"                 {GALLEON_EXPLORER}/address/{addr}")
        env_updates[env_key] = addr
    upsert_kaspa_env(env_updates, ROOT)
    print("saved game addresses in kaspa.env")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
