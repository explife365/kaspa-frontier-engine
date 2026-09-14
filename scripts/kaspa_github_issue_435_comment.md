Until kaspad exposes return-address on UTXO creation, we shipped a TN10 REST chain-walk shim in [kaspa-frontier-engine](https://github.com/explife365/kaspa-frontier-engine):

```
cargo run --release --bin tn10-return-address -- <deposit_txid> [vout]
```

Spec match: first input only → `Option<Address>`. Fails closed when parent tx is pruned (documents need for node-side UTXO diff lookup per issue description).

Live TN10 example (deposit vout 0):
- tx `cb0c084c3c6a4110f23bd1b8f580adf848b1d575e8bb3cd225dd71e46a8423f1`
- return `kaspatest:pzkrvy9mpwr7kn7mcaenta73ty8nnylu7adu8ghlh5h8fxpdwecgzlha07k54`

Willing to help review/test an upstream RPC implementation; our crate can swap to native when shipped (see `scripts/integrator_shims.py`).
