PR comment draft — `kaspanet/kaspa-python-sdk` #78 (`computeBudget` in `to_dict`)

**Posted 2026-09-07:** integrator fail-closed follow-up on PR (TN10 + #1074 boundary).
Earlier same-day comments cover local repro, unit tests, nested `Transaction` round-trip,
and fork CI `action_required`.

---
Pinned `kaspa` still drops that field on serialize, so covenant / SilverScript
broadcasts fail closed (effective budget 0 after RPC round-trip).

**Repro** (2.0.2rc1 and current `main` `convert.rs`):

```python
from kaspa import TransactionInput

ti = TransactionInput(
    outpoint={"transactionId": "00" * 32, "index": 0},
    signature_script=b"",
    sequence=0,
    sig_op_count=0,
    compute_budget=10,
)
assert ti.compute_budget == 10
d = ti.to_dict()
assert "computeBudget" not in d  # bug: field omitted
```

`from_dict` already honors `computeBudget` (defaults to 0 when omitted). The gap
is only `TryToPyDict` for `TransactionInput`:

```rust
dict.set_item("computeBudget", self.get_compute_budget())?;
```

**Test gap:** `test_input_from_dict_with_compute_budget_preserved` covers import
only. Need: construct with `compute_budget=10`, `to_dict()`, assert key present,
`from_dict` round-trip keeps 10.

**Not in scope here:** rusty-kaspa#1074 (WASM / wallet generator). That does not
fix Python `convert.rs`.

Local TN10 integrator work stays fail-closed until a published wheel includes this
fix. Not a kaspad change.
