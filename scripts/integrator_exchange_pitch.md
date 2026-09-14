# Kaspa TN10 integrator pilot — one page

**For:** exchange ops, bridge operators, custody vendors evaluating Kaspa listings.

## What you get

| Capability | Status |
|------------|--------|
| Multi-address UTXO ingestion (REST + owned wRPC) | Live on TN10 |
| DAA-depth deposit confirmation | Live |
| Exact withdrawal UTXO observation | Live |
| N-of-M owned node gate (2/2) | Live (evidence Sep 14 2026) |
| Durable deposit journal + webhook outbox | Live (rehearsal) |
| Deposit return-address (#435) | REST shim; native RPC pending |
| TX fee from enriched inputs (#615) | REST partial; GetBlocksV2 PR #906 |

## Why not public REST only

- No substitute for synced owned `kaspad --utxoindex`
- Reorg and lag checks require node health supervision
- Deposit credit must fail closed when gate is red

## Pilot offer

1. **Week 1:** Run your deposit addresses on our TN10 rehearsal stack; deliver evidence JSON + deposit journal export.
2. **Week 2:** Map your withdrawal flow to `tn10-withdraw` semantics; webhook dry-run to your staging endpoint.
3. **Week 3:** Mainnet cutover checklist (your nodes, your keys) — we do not hold mainnet custody.

## Pricing model

- Paid pilot in **KAS** or USD equivalent (integrator SOW, not a grant application)
- Optional ongoing: managed health monitoring + journal hosting

## Contact / repo

- Repository: https://github.com/explife365/kaspa-frontier-engine
- Evidence: `scripts/integrator_evidence_pack.ps1`
- Optional dev donation (mainnet KAS): `kaspa:qpxdemlyx445kt5xteux0qhadaw8lh5m0vnqvcy8fh483t70usgkkeulsx9cm`
