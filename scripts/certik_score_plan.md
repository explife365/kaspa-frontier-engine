# CertiK Skynet improvement plan — Kaspa (Foundation draft)

Independent integrator field notes for Kaspa Foundation / ops. **Not** a KIP, not a
consensus patch, and not a claim that Skynet score equals PoW security.

**Source:** [skynet.certik.com/projects/kaspa](https://skynet.certik.com/projects/kaspa) and
[skynet.certik.com/projects/bitcoin](https://skynet.certik.com/projects/bitcoin), snapshot
7 Sep 2026 EDT. Methodology:
[skynet.certik.com/skynet-score-methodology](https://skynet.certik.com/skynet-score-methodology).

---

## Current scores (evidence)

| Project | Skynet | Tier | ~5-star |
|---------|--------|------|---------|
| Kaspa | 84.59 | A | ~4.25 (aggregators round to 4.2) |
| Bitcoin | 97.53 | AAA | ~4.9 |

Reported Kaspa sub-scores (third-party aggregator, Apr–Aug 2026): Code Security
**91.61**, Market Stability **95.00**, Governance **81.70**, Operational Resilience
**76.68**, Fundamental Health **72.16**, Community Trust **72.45**.

**Neither** project shows “Audited by CertiK.” The gap is mostly **metadata, social
telemetry, maturity, and variable category weighting** — not “BTC has audited
consensus, Kaspa does not.”

---

## Public audits and security reviews (inventory)

What exists publicly today, what Skynet counts, and what **does not** cover
`rusty-kaspa` consensus.

| Report | Auditor / source | Scope | Covers L1 consensus? | CertiK / Skynet | Action |
|--------|------------------|-------|----------------------|-----------------|--------|
| KDX wallet v2.12.3 | [Y3TI / CCXD](https://y3ti.uk/audits/KASPA) (Jul 2023) | Legacy NodeJS desktop wallet + node bundle | **No** (wallet binary integrity) | Likely the **1** third-party audit on Skynet | Foundation: link on kaspa.org; note superseded by Rusty Kaspa wallet |
| `rusty-kaspa` / kaspad | — | Core GHOSTDAG + Toccata node | **No public third-party audit** | Blockworks TTF + QRI cite this gap | **Phase 1:** commission consensus audit (Trail of Bits, NCC Group, Least Authority, etc.) or Immunefi bounty |
| Go kaspad `sig_op_count` | Internal (Rust rewrite, 2022) | Historical malleability-class bug | Fixed; not a published PDF | Disclosure in [Blockworks TTF](https://blockworks.com/token-transparency/filing/kaspa) | Keep in transparency filing; no extra Skynet credit |
| SilverScript / `silverc` | Internal pre-v1 (e.g. PR #210 type audit, Aug 2026) | Covenant language compiler | **No** formal public report | Not listed | Publish audit scope + report before calling v1 production |
| Kasplex EVM L2 | [ScaleBit](https://github.com/kasplex/evm-l2-audit) | EVM rollup on Kaspa | **No** (L2 only) | Ecosystem, not native KAS L1 | Link from Kasplex docs; do not conflate with kaspad |
| LFG.KASPA / KaspaCom | [Cyberscope](https://www.cyberscope.io/audits/lfgkaspa) (Sep 2025) | L2 launchpad Solidity | **No** | Third-party smart-contract audit | Ecosystem only |
| Kaspa Nexus (KSPNX) | [Cyberscope](https://www.cyberscope.io/audits/kspnx) (2024) | Presale Solidity | **No** | Unrelated token project | Do not cite as Kaspa L1 audit |
| Igra KasExitBridge / KAS bridge | OpenZeppelin patterns; [integration docs](https://github.com/argonmining/igra-kas-bridge) | L2 bridge + UI | **No** | No Hacken/CertiK report found | Igra: external audit before mainnet scale; UI must keep WASM bech32 gate |
| Blockworks Token Transparency | Blockworks TTF (2026 H2) | Disclosure framework | Assessment, not crypto audit | Fundamental transparency | Foundation: complete allocation / launch fields Skynet marks “Not Available” |
| QRI quantum readiness | [qrindex.org](https://qrindex.org/projects/kaspa/) | Classical crypto exposure | Review, not implementation audit | Assurance caveat only | Not a substitute for consensus audit |

**Ratings aggregators** (not full audits): Cyberscope ~84/100, TokenInsight ~54/100 on
some dashboards — treat as **scores**, not `rusty-kaspa` proof.

### Actionable audit backlog (owners)

| # | Action | Owner | Unblocks | Verify |
|---|--------|-------|----------|--------|
| A1 | Submit Skynet “Missing info” + link **Y3TI report URL**, date, scope disclaimer | Foundation | Code Security audit history accuracy | Skynet 3rd-party audit detail page |
| A2 | Commission **rusty-kaspa** audit (Toccata + consensus); publish PDF on GitHub | Foundation + Core | Largest Code Security gap vs narrative | Public report + fixed-issue list |
| A3 | **Immunefi** or CertiK bounty for `kaspanet/rusty-kaspa` | Core + Foundation | Operational Resilience | Listed on Skynet + security.md |
| A4 | **SilverScript v1** external audit before “production” label | kaspanet/silverscript | Covenant UX / integrator safety | Public report; maps to `SILVERSCRIPT` experimental |
| A5 | Link **ScaleBit** L2 audit on kaspa.org ecosystem page (labeled L2) | Kasplex | Honest layering | No L1 conflation |
| A6 | Igra **KasExitBridge** third-party audit + on-chain checksum hardening roadmap | Igra | Bridge trust | Report + issue tracker |
| A7 | Integrator **evidence pack** (this crate) for CEX rehearsal | Integrators | `COMMUNITY_ASKS` CEX Partial | `scripts/integrator_evidence_pack.ps1` → `.local/evidence/` |
| A8 | Post **SDK #78** repro on `kaspa-python-sdk` PR (Toccata broadcast blocked) | Community | Covenant UX Partial | Maintainer CI green; see `scripts/sdk_pr78_comment.md` |

### Security review backlog (integrator crate, 7 Sep 2026)

Full diff review: **no medium+ vulnerabilities** in changed code. Residual and ecosystem items below.

| # | Finding | Severity | Owner | Status | Verify |
|---|---------|----------|-------|--------|--------|
| S1 | No public **rusty-kaspa** consensus audit | High (ecosystem) | Foundation + Core | Open | Same as A2/A3 |
| S2 | **Loopback kaspad trust** — wRPC notifications drive ledger; no node attestation | Low (rehearsal model) | Integrators | Documented | N-of-M + health gate; production needs independent nodes |
| S3 | **wRPC `is_ibd_peer` serde default** — missing field reads as false | Low | this crate | **Fixed** | `decode_connected_peer_info_response` requires explicit field per peer |
| S4 | **HTTP redirect** on webhook deliver could reach non-loopback target | Low | this crate | **Fixed** | `tn10-outbox` uses `redirect(Policy::none())` |
| S5 | **Loopback outbox receiver** cleartext HTTP — any local process can POST | Low | this crate | **Fixed** | mTLS default; `--allow-cleartext-loopback` for rehearsal |
| S6 | **`tn10_kaspad.ps1` P2P** binds `0.0.0.0:16211` (testnet) | Low (ops) | Ops | Open | Firewall / UPnP off; RPC stays loopback |
| S7 | **Igra KasExitBridge** on-chain address check is charset-only | Medium (L2) | Igra | Open | Same as A6; UI WASM bech32 gate is load-bearing |
| S8 | **Subscribe-ack journal** dropped deposits on restart | Medium | this crate | **Fixed** | `tn10-wrpc-live` tests `subscribe_ack_buffer_*` |
| S9 | **Missing `header_count`** with DAA > 0 could hide header/body gap | Low | this crate | **Fixed** | `owned_node` rejects zero header_count when DAA > 0 |
| S10 | Re-run security review before any production CEX cutover | Process | Integrators | **Done 8 Sep 2026** | Bugbot + security-review on uncommitted diff; clippy clean; 124 Rust + 74 Python pass |

---

## Why the contrast is sharp

CertiK applies **variable weights** per project. On the live Kaspa page (7 Sep 2026):

| Category | Kaspa weight | Bitcoin weight |
|----------|--------------|----------------|
| Community Trust | **45%** | 5% |
| Fundamental Health | **20%** | 5% |
| Operational Resilience | **15%** | 5% |
| Code / Governance / Market | ~5% each | ~5% each |

A weak Community Trust bucket costs Kaspa **~9×** more than it costs Bitcoin.

### Kaspa hindrances visible on Skynet today

| Signal | Kaspa | Bitcoin (reference) |
|--------|-------|---------------------|
| CertiK audit | No (1 third-party audit listed) | No |
| Team verification | Not verified | Not verified |
| Token launch date on Skynet | Not available | 15 yrs 8 mos |
| GitHub (Skynet) | 0 stars, medium impact | 101K+ stars, high impact |
| Twitter (Skynet) | N/A | 8.8M followers, very active |
| Bug bounty (Skynet) | None listed | None listed |
| Skynet Monitor | Not activated | Not activated |
| User rating (Skynet) | 7.64 (2,417) | 8.16 (64,772) |

Live L1 remains **GHOSTDAG @ 10 BPS + Toccata**. KIP-2 / DAGKnight stays
**Proposed**. Do not patch kaspad BPS or emit fake telemetry to chase score.

---

## Phased plan

### Phase 0 — Skynet hygiene (Foundation, 2–4 weeks)

| Action | CertiK bucket | Owner | Verify on Skynet |
|--------|---------------|-------|------------------|
| “Get Connected” + submit missing info | Fundamental + Community | Foundation | Launch date, listed date populated |
| Link official **GitHub org** (`kaspanet/rusty-kaspa`, wallets, docs) | Code + Operational | Foundation | Stars/activity ≠ 0 |
| Link official **X/Twitter** and socials | Community Trust | Foundation | Twitter N/A → live metrics |
| Activate **Skynet Monitor** (website, repo, social) | Operational | Foundation | Monitor tiles “Activated” |
| Publish **SECURITY.md** + disclosure contact on kaspa.org | Operational + Fundamental | Foundation + Core | Public URL |

### Phase 1 — Operational resilience (Core + Foundation, 1–3 months)

| Action | CertiK bucket | Owner | Verify |
|--------|---------------|-------|--------|
| **Bug bounty** (Immunefi or CertiK Bounty) for `rusty-kaspa` | Operational | Core + Foundation | Listed on Skynet |
| Document **incident response** + supported versions | Operational | Core | Linked from security page |
| Keep **third-party audit** trail public (scope, date) | Code | Foundation | Skynet audit history updated |
| Optional **team verification** package | Fundamental | Foundation | Team status on Skynet |

### Phase 2 — Ecosystem (community plan — maps to this crate’s `COMMUNITY_ASKS`)

These items build **integrator and exchange credibility**. They support Fundamental
“exchange operation” and Operational maturity; they do **not** replace Phase 0.

| Community ask | Crate status | Who can post / ship | Verification |
|---------------|--------------|---------------------|--------------|
| CEX plug-and-play REST wrapper | **Partial** | Integrators, exchanges | `tn10-node-health --min-healthy 2`, `tn10-deposits`, `tn10-withdraw`, `cargo test --lib` |
| Archival / indexer (UTXO + DAA) | **Shipped** | This crate | REST `/addresses/{}/utxos` + DAA depth tests |
| Local kaspad TN10 IBD | **Partial** | Node operators | Health fail-closes on DAA 0, IBD peers, header/body gap |
| Covenant UX lag | **Partial** | Wallet / explorer vendors | Toccata v1 on mainnet; explorers catch up |
| getUtxosByCovenantId | **Partial** | kascov + KIP path | `tn10-covenant-rpc` shim; not kaspad until KIP |
| Kasplex KRC-20 / Galleon gTEST | **Shipped** | L2/indexer teams | Existing TN10 + Galleon tests |
| DAGKnight / 100 BPS lore | **Refused** | — | Do not use for score chasing |
| kaspa-python-sdk #78 | **Blocked** | kaspanet maintainers | Toccata broadcast fail-closed until wheel ships |

### Phase 3 — Governance & market (ongoing)

| Track (`roadmap.rs`) | Status | CertiK relevance |
|----------------------|--------|------------------|
| GHOSTDAG @ 10 BPS | Live | Baseline protocol label |
| Toccata KIP-16/17/20/21 | Live | Code maturity narrative |
| KCC-0020 | Draft | Governance when ratified — not Kasplex KRC-20 |
| SilverScript / Argent | Experimental | Unaudited; proof = public txid only |
| DAGKnight KIP-2 | Research | Not activated |
| Binance/Coinbase spot (`L1_GAPS`) | Exchange custody | Market Stability + listings |

---

## What community volunteers can post (and what they cannot)

**Can help (evidence-only posts):**

- GitHub: repro + tests on `kaspa-python-sdk#78`, wallet Toccata gaps, rusty-kaspa issues
- KIP threads: research labeled “not a KIP verdict” (`scripts/kip2_*`)
- Discord / forum: TN10 IBD ops (`--appdir`, UTXO commit silent, no credits until healthy)
- Integrator demos: N-of-M health, DAA deposits, mTLS webhooks — with command output

**Cannot fix alone:**

- Skynet official account linkage, team verify, monitor activation — **Foundation only**
- Score campaigns without Phase 0 metadata — reads as spam, not security

Follow `.cursor/rules/public-posts.mdc`: progress, blockers, owners, numbers — no AI
attribution, no fake BPS/DAGKnight activation claims.

---

## Integrator evidence pack (this crate — reproducible)

Not a CertiK submission by itself; attach as **operational rehearsal** evidence if
Foundation requests integrator posture.

```text
# Community-ask board + L1 gap framing
cargo run --release --bin tn10-status

# Redundancy gate (fail-closed until 2/2 healthy)
cargo run --release --bin tn10-node-health -- \
  --url ws://127.0.0.1:18210 --url ws://127.0.0.1:28210 --min-healthy 2 --json

# Unit tests
cargo test --lib
pytest tests/ -q
```

Shipped controls worth citing: N-of-M supervisor health, DAA deposit/withdraw
tracking, IBD/header-gap/DAA-0 fail-closed, subscribe-then-REST-scan
(rusty-kaspa#939), ordered failover skipping unhealthy replicas, mTLS required for
non-loopback webhook delivery.

---

## Realistic trajectory

| Milestone | Expected Skynet lift | Notes |
|-----------|----------------------|-------|
| Phase 0 complete | +3–6 pts | Community + Fundamental telemetry |
| Phase 1 complete | +2–4 pts | Bounty + monitor + security docs |
| Phase 2 ecosystem maturity | Indirect | Exchange/listing desks, not Skynet formula |
| Match Bitcoin ~97 | Unlikely near-term | BTC wins on 15y liquidity, social scale, on-chain feeds |

Target **~88–92** (roughly 4.4–4.6 on 5-star scales) is plausible with Foundation
ops. **4.9** requires competing with the reference asset on maturity and market depth.

---

## Explicit non-goals

- Unofficial kaspad forks or BPS patches for ratings
- KIP-2 SAT fragments or 100 BPS sandbox results as “activation evidence”
- Claiming this integrator crate is kaspad or a substitute for Foundation Skynet onboarding
- Circle USDC / Uniswap on L1 (belongs on Igra/Kasplex L2 per `L1_GAPS`)

---

## Cross-references

- Roadmap tracks: `src/roadmap.rs` (`TRACKS`, `COMMUNITY_ASKS`, `L1_GAPS`)
- TN10 live plan canvas: `canvases/tn10-remaining-plan.canvas.tsx` (CertiK section)
- Public post drafts: `scripts/bps100_sandbox_post.md`, `scripts/kip2_thread_post.txt`,
  `scripts/sdk_compute_budget_issue.md`, `scripts/sdk_pr78_comment.md`
- Evidence runner: `scripts/integrator_evidence_pack.ps1`
- Foundation Skynet submission: `scripts/certik_foundation_submission.md`
