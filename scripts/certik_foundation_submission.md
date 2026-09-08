# Skynet “Get Connected” submission draft — Kaspa Foundation

**Foundation ops only.** This does not post to Discord or wake GitHub developers.
For community-facing copy use `scripts/kaspa_community_social_gap_post.txt` and
`scripts/kaspa_community_toccata_post.txt`. Skynet fixes aggregator metadata;
builder momentum stays on TN10 reproducible work and SDK PR reviews.

Copy-paste for [skynet.certik.com/projects/kaspa](https://skynet.certik.com/projects/kaspa) → **Get Connected** / **Missing info? Submit now**.

Independent integrator field notes. Not a KIP. Not consensus authority.

---

## Project identity

| Field | Value |
|-------|--------|
| Project | Kaspa (KAS) |
| L1 software | [kaspanet/rusty-kaspa](https://github.com/kaspanet/rusty-kaspa) (recommended node, v2.0.1+) |
| Live protocol | GHOSTDAG @ 10 BPS (Crescendo) + Toccata (KIP-16/17/20/21) |
| Website | https://kaspa.org |
| Docs | https://docs.kaspa.org |
| Testnet | TN10 (`--testnet --netsuffix=10`) |

## Token / launch metadata (currently “Not Available” on Skynet)

| Field | Public source |
|-------|----------------|
| Launch model | No ICO, no premine, no pre-sales ([Blockworks TTF](https://blockworks.com/token-transparency/filing/kaspa)) |
| Max supply | 28,704,026,601 KAS |
| Ticker | KAS |
| Recommended node | rusty-kaspa (Go kaspad deprecated) |

Foundation should supply exact **token launch date** and **listed date** in CertiK’s required format.

## Official repositories (fix Skynet GitHub “0 stars”)

Link these explicitly — not legacy org accounts:

- https://github.com/kaspanet/rusty-kaspa
- https://github.com/kaspanet/kips
- https://github.com/kaspanet/silverscript
- https://github.com/kaspanet/kaspa-python-sdk

## Official social (fix Twitter N/A)

Provide canonical X/Twitter, Discord, and Telegram URLs maintained by Foundation.

## Third-party audit on file (scope disclaimer)

| Report | Date | Scope | **Not** |
|--------|------|-------|---------|
| [Y3TI KDX](https://y3ti.uk/audits/KASPA) | Jul 2023 | Legacy KDX desktop wallet v2.12.3 integrity | rusty-kaspa consensus |
| [ScaleBit Kasplex L2](https://github.com/kasplex/evm-l2-audit) | 2025 | EVM L2 rollup | Native L1 kaspad |

**Gap:** No published third-party audit of `rusty-kaspa` consensus. Blockworks TTF and QRI note this. Foundation roadmap item: commission core audit + public bug bounty (Immunefi or CertiK Bounty).

## Skynet Monitor — activate

- Website: kaspa.org
- Code repository: kaspanet/rusty-kaspa
- Social: official X account

## Security contact (requested for Operational Resilience)

Publish on kaspa.org and link here:

- `SECURITY.md` in rusty-kaspa with disclosure email/process
- Supported release branches and response SLA

## Integrator evidence (ecosystem, not L1 audit)

Third-party TN10 integrator rehearsal (deposits/withdrawals DAA, N-of-M node health gate) — reproducible logs available on request. Does not substitute for core node audit.

Full plan: [`scripts/certik_score_plan.md`](https://github.com/explife365/kaspa-frontier-engine/blob/main/scripts/certik_score_plan.md) in [kaspa-frontier-engine](https://github.com/explife365/kaspa-frontier-engine) (public integrator repo). Evidence runner: `scripts/integrator_evidence_pack.ps1`.

---

## What not to submit

- Cyberscope L2 / presale token audits as “Kaspa L1 consensus” proof
- KIP-2 / DAGKnight / 100 BPS research as activation evidence
- Unofficial kaspad forks
