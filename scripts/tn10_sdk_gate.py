"""TN10 kaspa-python-sdk Toccata gate: computeBudget + SilverScript availability.

Exit 0 only when covenant broadcast is safe to attempt. Not a kaspad check.

  python scripts/tn10_sdk_gate.py
  python scripts/tn10_sdk_gate.py --json
  set TN10_SDK_DEV_PATCH=1 && python scripts/tn10_sdk_gate.py --dev --json
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from kaspa_sdk_dev_patch import (
    apply_dev_patch,
    dev_patch_applied,
    dev_patch_enabled,
    probe_compute_budget,
)

PR_URL = "https://github.com/kaspanet/kaspa-python-sdk/pull/78"


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
    parser.add_argument(
        "--dev",
        action="store_true",
        help="allow TN10_SDK_DEV_PATCH=1 rehearsal path (not production-ready)",
    )
    args = parser.parse_args()

    budget_ok_native, budget_msg_native, budget_value_native = probe_compute_budget()
    dev_patch = False
    budget_ok = budget_ok_native
    budget_msg = budget_msg_native
    budget_value = budget_value_native

    if not budget_ok_native and (args.dev or dev_patch_enabled()):
        dev_patch = apply_dev_patch()
        if dev_patch:
            budget_ok, budget_msg, budget_value = probe_compute_budget()
            if budget_ok:
                budget_msg = f"{budget_msg} (TN10 dev patch; not a published wheel)"

    ss_ok, ss_msg = check_silverscript()
    pr = check_pr_state()
    ready_native = budget_ok_native and ss_ok
    ready_dev = budget_ok and ss_ok and dev_patch
    ready = ready_native or (args.dev and ready_dev)

    report = {
        "ready": ready,
        "readyNative": ready_native,
        "readyWithDevPatch": ready_dev,
        "devPatchEnabled": dev_patch_enabled(),
        "devPatchApplied": dev_patch_applied(),
        "kaspaVersion": kaspa_version(),
        "computeBudgetOk": budget_ok,
        "computeBudgetOkNative": budget_ok_native,
        "computeBudgetMessage": budget_msg,
        "computeBudgetMessageNative": budget_msg_native,
        "computeBudgetSerialized": budget_value,
        "silverscriptOk": ss_ok,
        "silverscriptMessage": ss_msg,
        "pr78": pr,
        "pr78Url": PR_URL,
        "issue79Url": "https://github.com/kaspanet/kaspa-python-sdk/issues/79",
        "unblock": (
            "Merge kaspa-python-sdk#78 and install a published wheel that bundles "
            "SilverScript + computeBudget in TransactionInput.to_dict(). "
            "Interim TN10 rehearsal: set TN10_SDK_DEV_PATCH=1 and use --dev."
        ),
    }

    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"kaspa              {report['kaspaVersion']}")
        print(
            f"computeBudget      {'OK' if budget_ok_native else 'BLOCKED'} — {budget_msg_native}"
        )
        if dev_patch_applied():
            print(f"dev patch          active — {budget_msg}")
        print(f"silverscript       {'OK' if ss_ok else 'BLOCKED'} — {ss_msg}")
        if pr.get("available"):
            print(
                f"PR #78             {pr.get('state')} mergeable={pr.get('mergeable')} "
                f"review={pr.get('reviewDecision') or 'pending'}"
            )
        else:
            print(f"PR #78             {pr.get('note') or pr.get('error', 'unknown')}")
        label = "READY" if ready_native else ("READY (dev patch)" if ready_dev and args.dev else "FAIL-CLOSED")
        print(f"covenant broadcast {label}")
        if not ready:
            print(f"unblock            {report['unblock']}")

    if ready_native:
        return 0
    if args.dev and ready_dev:
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main())
