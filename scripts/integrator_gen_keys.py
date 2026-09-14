#!/usr/bin/env python3
"""Generate integrator API keys into gitignored kaspa.env.

Does not print secret values. Pilot credentials are also written to
.local/integrator_pilot_credentials.txt (gitignored) for CEX handoff.

  python scripts/integrator_gen_keys.py
  python scripts/integrator_gen_keys.py --force
"""

from __future__ import annotations

import argparse
import secrets
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from kaspa_env import env_path, upsert_kaspa_env  # noqa: E402

PLACEHOLDER_MARKERS = (
    "change-me",
    "replace-with",
    "your-long-random",
    "another-key",
)


def _existing_keys(path: Path) -> str | None:
    if not path.is_file():
        return None
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line.startswith("INTEGRATOR_API_KEYS="):
            return line.partition("=")[2].strip()
    return None


def _looks_placeholder(value: str | None) -> bool:
    if not value:
        return True
    lowered = value.lower()
    return any(marker in lowered for marker in PLACEHOLDER_MARKERS)


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate TN10 integrator API keys")
    parser.add_argument(
        "--force",
        action="store_true",
        help="rotate keys even when kaspa.env already has non-placeholder values",
    )
    args = parser.parse_args()

    path = env_path()
    current = _existing_keys(path)
    if current and not _looks_placeholder(current) and not args.force:
        print("INTEGRATOR_API_KEYS already set in kaspa.env (use --force to rotate)")
        return 0

    pilot = secrets.token_urlsafe(32)
    cex_demo = secrets.token_urlsafe(32)
    webhook = secrets.token_hex(32)
    upsert_kaspa_env(
        {
            "INTEGRATOR_API_BIND": "127.0.0.1:8787",
            "INTEGRATOR_API_KEYS": f"pilot:{pilot},cex-demo:{cex_demo}",
            "INTEGRATOR_WEBHOOK_SECRET": webhook,
            "INTEGRATOR_REQUIRE_GATE": "1",
            "INTEGRATOR_CONFIRMATION_DAA": "10",
            "TN10_DEPOSIT_DATABASE": ".local/tn10-wrpc-live.sqlite",
            "TN10_WITHDRAWAL_DATABASE": ".local/tn10-withdrawals.sqlite",
            "TN10_OWNED_NODE_URLS": "ws://127.0.0.1:18210,ws://127.0.0.1:28210",
            "TN10_MIN_HEALTHY": "2",
        }
    )

    cred_path = ROOT / ".local" / "integrator_pilot_credentials.txt"
    cred_path.parent.mkdir(parents=True, exist_ok=True)
    cred_path.write_text(
        "\n".join(
            [
                "# TN10 integrator pilot credentials — never commit",
                f"pilot_key={pilot}",
                f"cex_demo_key={cex_demo}",
                f"webhook_secret={webhook}",
                "header=X-Integrator-Key",
                "tenants=pilot,cex-demo",
                "",
            ]
        ),
        encoding="utf-8",
    )
    print("Integrator keys upserted in kaspa.env")
    print(f"CEX handoff copy: {cred_path}")
    print("Tenants: pilot (primary), cex-demo (staging)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
