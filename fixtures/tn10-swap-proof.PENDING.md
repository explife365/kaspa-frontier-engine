# Restricted swap proof bundle (pending on-chain broadcast)

Public txids are **not** checked in yet. Same blocker as vault/counter:
[kaspa-python-sdk#78](https://github.com/kaspanet/kaspa-python-sdk/pull/78).

## Ship steps (after SDK #78)

```powershell
python examples/silverscript/restricted_swap.py --print-address
python examples/silverscript/restricted_swap.py
python examples/silverscript/restricted_swap.py --publish-fixture

cargo run --release --bin tn10-proof -- fixtures/tn10-swap-proof.json --capture-fixtures
cargo run --release --bin tn10-proof -- fixtures/tn10-swap-proof.json --offline --json
```

Pairs with off-chain destination checks in `src/covenant.rs`.
