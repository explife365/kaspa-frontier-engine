"""One entry point for Kaspa app developers — pick a path, get exact commands.

  python examples/dev_quickstart.py
  python examples/dev_quickstart.py --path rest
  python examples/dev_quickstart.py --path galleon
  python examples/dev_quickstart.py --path nodes
  python examples/dev_quickstart.py --path covenant
  python examples/dev_quickstart.py --json

Not consensus. Fail-closed per path.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from kaspa_env import load_kaspa_env  # noqa: E402

PATHS: dict[str, dict[str, Any]] = {
    "rest": {
        "title": "No disk — REST + offline proofs",
        "requires": "Internet only",
        "minutes": 5,
        "steps": [
            "python scripts/tn10_adoption_scorecard.py --public-only",
            "python examples/parity_quest.py --rounds 3",
            "cargo run --release --bin tn10-proof -- fixtures/tn10-counter-proof.json --offline",
            "python scripts/integrator_shims.py --json",
        ],
        "media": {
            "text": "docs/dev_build_faster.md",
            "video": "scripts/media/dev_video_script.txt",
            "audio": "scripts/media/dev_audio_script.txt",
            "dashboard": "examples/kaspa_frontier_dashboard.html",
        },
        "blocker_note": "Do not credit deposits — public REST is read-only yellow path.",
    },
    "galleon": {
        "title": "Galleon L2 — games and DeFi rehearsal",
        "requires": "GALLEON_PRIVATE_KEY + faucet iKAS (examples/galleon_faucet.py --drip)",
        "minutes": 15,
        "steps": [
            "python examples/galleon_faucet.py --ensure-wallet",
            "python examples/galleon_faucet.py --drip",
            "python examples/galleon_games.py list",
            "python examples/galleon_games_deploy.py --simulate",
            "python examples/galleon_games.py coin-flip --heads --bet 0.1 --dry-run",
            "python examples/galleon_pool.py --status",
            "python examples/galleon_dex.py status",
            "python examples/galleon_dex.py quote --sell 1.0 --buy wiKAS",
            "python examples/integrator_api.py --port 8788",
            "curl http://127.0.0.1:8788/v1/cex/validate",
        ],
        "media": {
            "text": "examples/galleon/README.md",
            "video": "scripts/media/dev_video_script.txt",
        },
        "blocker_note": "gTEST is not USD. On-chain randomness is demo-grade.",
    },
    "nodes": {
        "title": "Owned kaspad — exchange-grade ingestion",
        "requires": "~450G disk, TN10 IBD, --utxoindex",
        "minutes": 60,
        "steps": [
            "powershell -File scripts/tn10_node_onboard.ps1",
            "python scripts/tn10_adoption_scorecard.py --json",
            "python examples/integrator_status.py",
            "powershell -File scripts/integrator_evidence_pack.ps1",
        ],
        "media": {"text": "docs/dev_build_faster.md"},
        "blocker_note": "N-of-M gate must be green before deposit credit.",
    },
    "covenant": {
        "title": "L1 covenant app — SilverScript on TN10",
        "requires": "KASPA_TN10_FUNDING_KEY + TN10_SDK_DEV_PATCH=1",
        "minutes": 30,
        "steps": [
            "python scripts/tn10_sdk_gate.py --json",
            "python examples/silverscript/counter.py --print-address",
            "python examples/silverscript/counter.py",
            "python examples/integrator_status.py --skip-gate",
        ],
        "media": {"text": "docs/dev_build_faster.md"},
        "blocker_note": "Native wheel: kaspa-python-sdk#78. Covenant index: rusty-kaspa#1128.",
    },
}


def detect_path() -> str:
    load_kaspa_env(ROOT)
    if os.environ.get("KASPA_TN10_FUNDING_KEY") or os.environ.get("KASPA_FUNDING_KEY"):
        return "covenant"
    if os.environ.get("GALLEON_PRIVATE_KEY"):
        return "galleon"
    return "rest"


def build_card(path_id: str) -> dict[str, Any]:
    card = PATHS[path_id].copy()
    card["id"] = path_id
    card["kaspa_leaders"] = [
        "GHOSTDAG @ 10 BPS — parallel blocks, not one tip",
        "Toccata covenants live on L1 (no EVM on kaspad)",
        "Shim layer swaps to native RPC/SDK without app rewrite",
    ]
    return card


def print_human(path_id: str) -> None:
    card = build_card(path_id)
    print(f"\n=== Kaspa dev quickstart: {card['title']} ===\n")
    print(f"Time estimate  ~{card['minutes']} min")
    print(f"Requires       {card['requires']}\n")
    print("Why Kaspa leads for apps:")
    for line in card["kaspa_leaders"]:
        print(f"  • {line}")
    print("\nCommands (in order):")
    for i, step in enumerate(card["steps"], 1):
        print(f"  {i}. {step}")
    print(f"\nNote: {card['blocker_note']}")
    media = card.get("media") or {}
    if media:
        print("\nLearn more (text / video / audio):")
        for kind, rel in media.items():
            print(f"  {kind:8} {rel}")
    print("\nAPI: python examples/integrator_api.py  ->  GET /v1/onboard?path=" + path_id)


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Kaspa developer quickstart router")
    parser.add_argument("--path", choices=list(PATHS.keys()), help="developer path")
    parser.add_argument("--list", action="store_true", help="list all paths")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    if args.list:
        body = {k: {"title": v["title"], "minutes": v["minutes"]} for k, v in PATHS.items()}
        if args.json:
            print(json.dumps(body, indent=2))
        else:
            for pid, meta in body.items():
                print(f"  {pid:10} {meta['title']} (~{meta['minutes']} min)")
        return 0

    path_id = args.path or detect_path()
    if args.json:
        print(json.dumps(build_card(path_id), indent=2))
    else:
        if not args.path:
            print(f"Auto-selected path: {path_id} (use --path to override)\n")
        print_human(path_id)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
