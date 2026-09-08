"""TN10-only dev patch for kaspa-python-sdk#78 until a published wheel ships.

When TN10_SDK_DEV_PATCH=1, wraps TransactionInput.to_dict() so computeBudget
is preserved on the RPC JSON round-trip. Testnet rehearsal only — not a
production workaround and not a substitute for merging PR #78.

  set TN10_SDK_DEV_PATCH=1
  python scripts/tn10_sdk_gate.py --dev --json
"""

from __future__ import annotations

import os

COMPUTE_BUDGET_PROBE = 10
_ENV_FLAG = "TN10_SDK_DEV_PATCH"
_PATCH_ATTR = "_tn10_dev_patch_applied"


def dev_patch_enabled() -> bool:
    return os.environ.get(_ENV_FLAG, "").strip() in {"1", "true", "yes", "on"}


def dev_patch_applied() -> bool:
    try:
        from kaspa import TransactionInput

        return bool(getattr(TransactionInput, _PATCH_ATTR, False))
    except ImportError:
        return False


def apply_dev_patch() -> bool:
    """Patch TransactionInput.to_dict once. Returns True if patch is active."""
    if not dev_patch_enabled():
        return False
    try:
        from kaspa import TransactionInput
    except ImportError:
        return False
    if getattr(TransactionInput, _PATCH_ATTR, False):
        return True

    original = TransactionInput.to_dict

    def patched_to_dict(self):  # type: ignore[no-untyped-def]
        data = original(self)
        if not isinstance(data, dict):
            return data
        budget = getattr(self, "compute_budget", None)
        if budget is None:
            return data
        patched = dict(data)
        patched["computeBudget"] = budget
        return patched

    TransactionInput.to_dict = patched_to_dict  # type: ignore[method-assign]
    setattr(TransactionInput, _PATCH_ATTR, True)
    return True


def ensure_dev_patch_if_enabled() -> bool:
    return apply_dev_patch()


def probe_compute_budget() -> tuple[bool, str, int | None]:
    from kaspa import Hash, TransactionInput, TransactionOutpoint

    probe = TransactionInput(
        TransactionOutpoint(Hash("00" * 32), 0),
        b"",
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET_PROBE,
    )
    encoded = probe.to_dict()
    value = encoded.get("computeBudget")
    if value == COMPUTE_BUDGET_PROBE:
        return True, "computeBudget preserved on to_dict", COMPUTE_BUDGET_PROBE
    return (
        False,
        "computeBudget dropped on to_dict (effective budget 0 after RPC round-trip)",
        value if isinstance(value, int) else None,
    )
