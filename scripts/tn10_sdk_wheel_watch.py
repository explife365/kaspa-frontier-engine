#!/usr/bin/env python3
"""Poll PyPI + SDK gate until kaspa-python-sdk#78 ships in a published wheel."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GATE = ROOT / "scripts" / "tn10_sdk_gate.py"
PYPI = "https://pypi.org/pypi/kaspa/json"


def pypi_version() -> str:
    with urllib.request.urlopen(PYPI, timeout=20) as resp:
        data = json.load(resp)
    return str(data["info"]["version"])


def gate_report() -> dict:
    proc = subprocess.run(
        [sys.executable, str(GATE), "--json"],
        capture_output=True,
        text=True,
        cwd=ROOT,
    )
    report = json.loads(proc.stdout)
    report["_exitCode"] = proc.returncode
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description="Watch for published kaspa wheel with PR #78 fix")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--interval", type=int, default=3600, help="seconds between polls (default 3600)")
    parser.add_argument("--once", action="store_true", help="single check, no loop")
    args = parser.parse_args()

    while True:
        version = pypi_version()
        report = gate_report()
        payload = {
            "pypiVersion": version,
            "readyNative": report.get("readyNative", report.get("ready")),
            "computeBudgetOkNative": report.get("computeBudgetOkNative", report.get("computeBudgetOk")),
            "kaspaVersion": report.get("kaspaVersion"),
            "pr78": report.get("pr78"),
        }
        if args.json:
            print(json.dumps(payload, indent=2))
        else:
            print(f"PyPI kaspa {version}  installed {report.get('kaspaVersion')}")
            print(
                f"native computeBudget {'OK' if payload['computeBudgetOkNative'] else 'BLOCKED'}  "
                f"readyNative={payload['readyNative']}"
            )
            if payload["readyNative"]:
                print("published wheel ready — run: powershell -File scripts/tn10_fixture_publish_when_ready.ps1")

        if payload["readyNative"]:
            return 0
        if args.once:
            return 1
        time.sleep(max(args.interval, 60))


if __name__ == "__main__":
    raise SystemExit(main())
