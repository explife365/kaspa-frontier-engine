"""Run CEX + DEX integrator API scenarios (offline + live).

  python examples/cex_api_validate.py
  python examples/cex_api_validate.py --json
  python examples/cex_api_validate.py --with-gate
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from cex_api_common import validate_scenarios  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="CEX/DEX API scenario validation")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--with-gate", action="store_true", help="include cargo node-health in readiness")
    parser.add_argument("--no-dex", action="store_true", help="skip live DEX RPC scenarios")
    args = parser.parse_args()

    report = validate_scenarios(skip_gate=not args.with_gate, live_dex=not args.no_dex)
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"CEX/DEX scenarios  {report['passed']}/{report['total']} ok\n")
        for row in report["scenarios"]:
            mark = "ok" if row["ok"] else "FAIL"
            print(f"  {mark:4}  {row['scenario']:28}  {row['latency_ms']}ms")
            if not row["ok"]:
                err = row.get("body", {}).get("error")
                if err:
                    print(f"         {err}")
        print(f"\nbundle {'green' if report['ok'] else 'red'}")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
