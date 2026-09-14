Integrator rehearsal from [kaspa-frontier-engine](https://github.com/explife365/kaspa-frontier-engine): exchange deposit tooling needs enriched input UTXOs on owned nodes, not only public REST.

`GetVirtualChainFromBlockV2` covers part of this; `GetBlocksV2` ([#906](https://github.com/kaspanet/rusty-kaspa/pull/906)) matches our fee + source-address use cases for block-scoped ingestion.

Evidence: TN10 2/2 owned-node gate + wRPC deposit journal (Sep 14 2026).
Gist: https://gist.github.com/explife365/477afea386ddba43574c7cb841ad4c73

CLI fee probe: `cargo run --release --bin tn10-return-address -- <txid> --fee`

Happy to run integration tests against a node build that includes #906 when available.
