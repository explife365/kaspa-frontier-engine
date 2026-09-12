## TN10 integrator demand signal (not consensus)

Cross-posting evidence from TN10 covenant rehearsal that motivates native `getUtxosByCovenantId` on kaspad RPC ([#1128](https://github.com/kaspanet/rusty-kaspa/issues/1128)).

**App-side demand (community, Sep 2026)**

- **DD12** even/odd commit–reveal game on TN10 — needs covenant-scoped UTXO discovery for playable loops, not address scans.
- **KRON / Hashlock audit thread** — auditability improves when integrators can reconcile covenant locks from node truth, not only third-party indexers.

**Integrator rehearsal status** ([kaspa-frontier-engine](https://github.com/explife365/kaspa-frontier-engine), testnet-10 only)

| Check | Status |
|-------|--------|
| Covenant fixture offline verify | 8/8 (`integrator_status.py`) |
| Owned-node N-of-M ingestion gate | 2/2 green (`tn10_adoption_scorecard.py`) |
| Public REST-only dev path | `--public-only` scorecard (read-only; no deposit credit) |
| Interim shim | `tn10-covenant-rpc` + kascov; labeled fail-closed where unverified |

**KIP outline (draft, not submitted):** `scripts/getUtxosByCovenantId_kip_outline.md` in the repo above — proposes RPC shape mirroring `getUtxosByAddresses`, `--utxoindex` prerequisite, pagination, explicit `verified` flag, no silent third-party proxy from kaspad.

**Ask:** feedback on response shape (`RpcUtxoEntry` vs minimal fields), covenant ID normalization, and default page limits before implementation.

Interim path remains required until this ships; happy to adjust the outline from maintainer comments.
