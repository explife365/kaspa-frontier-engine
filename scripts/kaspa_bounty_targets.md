# Kaspa reward targets (Sep 2026)

Honest map of where KAS compensation exists vs where this repo already has leverage.
Not financial advice. Testnet work does not mint mainnet KAS.

## Closed (do not chase)

| Program | Status | Notes |
| --- | --- | --- |
| **Kaspathon 2026** | Finalized | 200,000 KAS pool; winners at [kaspathon.com](https://kaspathon.com/). Deadline was Feb 15, 2026. |
| **Imperial College AI hackathon** | Sponsored | Community vote approved ~$25k KAS sponsorship; event-specific. |

## Open paths (realistic)

### 1. Community funding (Discord) — best fit for integrator work

Kaspa has **no standing paid dev roster**. Compensation flows through:

1. Post in `#votes-and-funding-discussions` on [Kaspa Discord](https://discord.gg/Kaspa)
2. Preliminary vote → general vote → fundraiser to public dev fund (2/4 multisig treasurers)
3. Milestone-based release

**Pitch angle for this repo:** TN10 CEX/integrator rehearsal — owned-node N-of-M gate, wRPC deposit journal, durable outbox, covenant proof tooling. Aligns with [kaspa.org integrator guide](https://kaspa.org/integration-of-kaspa-blockdag-guide/) and exchange deposit/withdraw asks.

**Deliverable to attach:** `powershell -File scripts/integrator_evidence_pack.ps1` → `.local/evidence/evidence_*.json`

### 2. Paid integrator / exchange services (not a bounty, but mainnet KAS income)

| Buyer | What they need | This crate |
| --- | --- | --- |
| Exchanges listing KAS | Deposit detection, reorg safety, withdrawal observation | `tn10-wrpc-live`, `tn10-deposits`, `tn10-withdraw`, N-of-M gate |
| Bridge / L2 (Igra, Kasplex) | L1 entry monitoring, HTLC paths | Galleon bridge relayer (testnet proof) |
| Indexers (kascov) | Covenant UTXO queries | `tn10-covenant-rpc` shim |

**Go-to-market:** ship evidence pack + one-page runbook (`scripts/cex_production_runbook.md`), offer TN10 pilot → mainnet cutover after their nodes.

### 3. Core / rusty-kaspa enhancements (indirect; may unlock grants)

Open issues integrators care about (no $ label on GitHub, but high community value):

| Issue | Repo | Fit |
| --- | --- | --- |
| [#615](https://github.com/kaspanet/rusty-kaspa/issues/615) Full TX in GetVirtualChainFromPath / GetBlocks | rusty-kaspa | Reduces external API dependency for fee/source-address derivation |
| [#435](https://github.com/kaspanet/rusty-kaspa/issues/435) Return address for UTXO | rusty-kaspa | Deposit attribution for exchanges |
| [#659](https://github.com/kaspanet/rusty-kaspa/issues/659) get_utxos over RPC | rusty-kaspa | Wallet/indexer ergonomics |

Contributing a reviewed PR here is **reputation + retroactive funding** territory (Discord vote), not instant payout.

### 4. Security bounties (not yet live for Kaspa core)

`scripts/certik_score_plan.md` documents the gap: **no public Immunefi/CertiK bounty** on `rusty-kaspa` today. Foundation roadmap item — not actionable until listed.

### 5. KEF / ecosystem grants

[Kaspa Ecosystem Foundation](https://kaspafaq.com/faq/contribute/) supports research grants strategically. Process is relationship + proposal, not a public application form with fixed bounties.

### 6. Build + monetize on Toccata / L2 (post-Toccata mainnet)

When Toccata activates on mainnet (2026 narrative): payments, gaming, real-time data apps can earn **usage fees** in KAS/KRC-20 — not grants. Your Galleon games + FeePool stack is rehearsal for **product revenue**, not faucet farming.

## Recommended build order (this repo → KAS)

1. **Done:** 2/2 TN10 adoption gate green
2. **Now:** `integrator_evidence_pack.ps1` with live wRPC journal + outbox
3. **Next:** Record 5-min demo (`scripts/media/dev_video_script.txt`) + publish evidence JSON
4. **Pitch:** Discord funding post — "TN10 integrator custody journal, N-of-M, fail-closed"
5. **Parallel:** Outreach to small exchanges / OTC desks testing Kaspa deposits (paid pilot)
6. **Optional:** PR to rusty-kaspa #615 or #435 if you want core credibility

## What not to build for "bounty"

- Fake DAGKnight / 100 BPS telemetry (refused in this crate)
- Unofficial kaspad consensus patches
- Another testnet faucet bot (no mainnet path)
- Kaspathon re-submission (closed)
