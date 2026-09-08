# Timelock vault proof bundle (pending on-chain broadcast)

Public txids are **not** checked in yet. Broadcast is blocked until
[kaspa-python-sdk#78](https://github.com/kaspanet/kaspa-python-sdk/pull/78) ships
in a published wheel.

## Ship steps (after SDK #78)

```powershell
python examples/silverscript/timelock_vault.py --print-address
# fund kaspatest: address from faucet-tn10.kaspanet.io

python examples/silverscript/timelock_vault.py
# genesis + DAA wait + release

python examples/silverscript/timelock_vault.py --publish-fixture
# copies .local/tn10-vault-proof.json -> fixtures/tn10-vault-proof.json

cargo run --release --bin tn10-proof -- fixtures/tn10-vault-proof.json --capture-fixtures
cargo run --release --bin tn10-proof -- fixtures/tn10-vault-proof.json --offline --json
cargo run --release --bin tn10-proof -- fixtures/tn10-vault-proof.json --kascov-only --json
```

Counter reference (existing public txids, REST pruned): `fixtures/tn10-counter-proof.json`.
