"""TN10 kaspa-python-sdk Toccata gate: computeBudget + SilverScript availability.

Exit 0 only when covenant broadcast is safe to attempt. Not a kaspad check.

  python scripts/tn10_sdk_gate.py
  python scripts/tn10_sdk_gate.py --json
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

COMPUTE_BUDGET = 10
PR_URL = "https://github.com/kaspanet/kaspa-python-sdk/pull/78"


def check_compute_budget() -> tuple[bool, str, int | None]:
    try:
        from kaspa import Hash, TransactionInput, TransactionOutpoint
    except ImportError as exc:
        return False, f"kaspa import failed: {exc}", None

    probe = TransactionInput(
        TransactionOutpoint(Hash("00" * 32), 0),
        b"",
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
    )
    encoded = probe.to_dict()
    value = encoded.get("computeBudget")
    if value == COMPUTE_BUDGET:
        return True, "computeBudget preserved on to_dict", COMPUTE_BUDGET
    return (
        False,
        "computeBudget dropped on to_dict (effective budget 0 after RPC round-trip)",
        value if isinstance(value, int) else None,
    )


def check_silverscript() -> tuple[bool, str]:
    try:
        import kaspa.experimental.silverscript as silverscript  # noqa: F401
    except ImportError as exc:
        return False, f"silverscript missing: {exc}"
    return True, "silverscript native extension loaded"


def check_pr_state() -> dict:
    if not shutil.which("gh"):
        return {"available": False, "note": "gh CLI not installed"}
    try:
        raw = subprocess.check_output(
            [
                "gh",
                "pr",
                "view",
                "78",
                "--repo",
                "kaspanet/kaspa-python-sdk",
                "--json",
                "state,mergeable,reviewDecision,url",
            ],
            text=True,
            timeout=20,
        )
        data = json.loads(raw)
        data["available"] = True
        return data
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired, json.JSONDecodeError) as exc:
        return {"available": False, "error": str(exc)}


def kaspa_version() -> str:
    try:
        from importlib.metadata import version

        return version("kaspa")
    except Exception:
        return "unknown"


def main() -> int:
    parser = argparse.ArgumentParser(description="TN10 SDK Toccata broadcast gate")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    budget_ok, budget_msg, budget_value = check_compute_budget()
    ss_ok, ss_msg = check_silverscript()
    pr = check_pr_state()
    ready = budget_ok and ss_ok

    report = {
        "ready": ready,
        "kaspaVersion": kaspa_version(),
        "computeBudgetOk": budget_ok,
        "computeBudgetMessage": budget_msg,
        "computeBudgetSerialized": budget_value,
        "silverscriptOk": ss_ok,
        "silverscriptMessage": ss_msg,
        "pr78": pr,
        "pr78Url": PR_URL,
        "unblock": (
            "Merge kaspa-python-sdk#78 and install a published wheel that bundles "
            "SilverScript + computeBudget in TransactionInput.to_dict(). "
            "Do not pip install from git on Windows without the SilverScript injection step."
        ),
    }

    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"kaspa              {report['kaspaVersion']}")
        print(f"computeBudget      {'OK' if budget_ok else 'BLOCKED'} — {budget_msg}")
        print(f"silverscript       {'OK' if ss_ok else 'BLOCKED'} — {ss_msg}")
        if pr.get("available"):
            print(
                f"PR #78             {pr.get('state')} mergeable={pr.get('mergeable')} "
                f"review={pr.get('reviewDecision') or 'pending'}"
            )
        else:
            print(f"PR #78             {pr.get('note') or pr.get('error', 'unknown')}")
        print(f"covenant broadcast {'READY' if ready else 'FAIL-CLOSED'}")
        if not ready:
            print(f"unblock            {report['unblock']}")

    return 0 if ready else 1


if __name__ == "__main__":
    sys.exit(main())
