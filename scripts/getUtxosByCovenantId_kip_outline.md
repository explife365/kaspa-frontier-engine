# KIP outline: `getUtxosByCovenantId` on kaspad RPC

Draft for community review. Not submitted to kaspanet/kips. Not consensus until accepted.

## Problem

Toccata covenants are live. Integrators need UTXOs locked under a `covenant_id` without scanning all addresses. kaspad exposes `getUtxosByAddresses` but not covenant-scoped lookup.

Today integrators use community indexers (e.g. kascov) or local shims (`tn10-covenant-rpc` in [kaspa-frontier-engine](https://github.com/explife365/kaspa-frontier-engine)).

## Proposed RPC

```json
{
  "method": "getUtxosByCovenantId",
  "params": {
    "covenantId": "<32-byte hex>",
    "includeMempool": false
  }
}
```

Response shape mirrors `getUtxosByAddresses` entries, with an explicit `verified: bool` when the node cannot attest indexer-sourced rows.

## Semantics

- **Source of truth:** node UTXO set with `--utxoindex` (same prerequisite as address lookup).
- **Mempool:** optional; default false for custody integrations.
- **Pagination:** `limit` + `offset` or cursor — covenant UTXO sets can be large on TN10.
- **Fail closed:** return error if `--utxoindex` disabled; do not silently proxy third-party indexers from kaspad.

## Non-goals

- Not a replacement for kascov lineage/history APIs.
- Not token metadata (KCC-0020 draft is separate).
- Not a shortcut to skip `--utxoindex` on exchanges.

## Interim integrator path

Until native RPC ships:

1. `tn10-covenant-rpc` — loopback Axum shim over public REST + kascov (bounded, labeled unverified where applicable).
2. `tn10-proof --kascov-only` — verify covenant lineage when REST prunes historical txids.

## Open questions for Core

1. Should response include `scriptPublicKey` + `amount` only, or full `RpcUtxoEntry`?
2. Covenant ID normalization (hex case, `0x` prefix)?
3. Rate limits / max page size defaults for public RPC?

## References

- Toccata guide: output `covenant_id`, input `compute_budget`
- Integrator rehearsal: `scripts/tn10_covenant_rpc_smoke.ps1`
- Community indexer: https://kascov.io/data/testnet-10/c/<id>.json
