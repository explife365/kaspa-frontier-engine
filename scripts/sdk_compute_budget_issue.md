Highest-value integrator contribution (not a consensus patch)

Target: https://github.com/kaspanet/kaspa-python-sdk
Not rusty-kaspa consensus. Not a kaspad fork. Not 100 BPS.

Toccata is live. kaspad already accepts v1 txs with input computeBudget.
Pinned/published Python `kaspa` still drops that field on serialize, so
covenant and SilverScript broadcasts fail closed (script units exceeded,
effective budget 0).

Live check against current `kaspa` (2.0.2rc1) and against
`kaspanet/kaspa-python-sdk` main `src/consensus/convert.rs`:

    ti = TransactionInput(outpoint, b"", sequence=0, sig_op_count=0, compute_budget=10)
    ti.compute_budget          # 10  — present in memory
    sorted(ti.to_dict().keys())
    # ['previousOutpoint', 'sequence', 'sigOpCount', 'signatureScript', 'utxo']
    # computeBudget is missing

from_dict already honors `computeBudget` (and defaults to 0 when omitted —
good for v0 JSON). to_dict never emits it, so any submit path that
round-trips through the dict/JSON RPC shape silently zeros the budget.

The missing line in `impl TryToPyDict for TransactionInput` is:

    dict.set_item("computeBudget", self.get_compute_budget())?;

Unit-test gap: `test_input_from_dict_with_compute_budget_preserved` only
covers from_dict. `test_input_from_dict_roundtrip` uses default budget 0,
so it still passes. Needed: construct with compute_budget=10, to_dict(),
assert key == 10, from_dict round-trip keeps 10.

Related but separate: rusty-kaspa#1074 (WASM / wallet generator /
ComputeCommit). That PR does not fix Python convert.rs. A published
kaspa-python-sdk wheel with the one-line to_dict fix unblocks TN10
covenant broadcasts without an unofficial SDK.

Do not treat this as a KIP or a kaspad change.
