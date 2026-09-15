# CEX outreach status (updated Sep 15 2026)

**Daily audit:** `powershell -File scripts/cex_outreach/check_channels.ps1`

From: explife365@gmail.com

| Exchange | Channel | Status | Notes |
|----------|---------|--------|-------|
| Gate.io | listing@gate.io | **Sent** | Sep 14 2026 |
| Bybit | listing@bybit.com | **Sent** | Sep 14 2026; earlier bounce — retry may not deliver; keep LinkedIn backup |
| MEXC | listing@mexc.com | **Sent** | Sep 14 2026 |
| KuCoin | listing@kucoin.com | **Sent** | Sep 14 2026 |

## Bybit — why blocked

`listing@bybit.com` sits behind strict enterprise mail policy. Cold outreach from consumer Gmail is often rejected before delivery. This is normal for large CEX listing inboxes — not a spam score issue on your side.

## Bybit alternate channels (try in order)

1. **Bybit institutional / VIP / API business form** — https://www.bybit.com/en/help-center/ (search “institutional” or “API business”)
2. **LinkedIn** — message Bybit integrations or wallet ops contacts with the short pitch (`bybit_linkedin.txt`)
3. **Kaspa Foundation / ecosystem intro** — ask in Discord #development if anyone has a warm Bybit integrations contact
4. **Resend from a domain address** — if you have `@yourdomain.com` on Google Workspace, some CEX filters trust it more than `@gmail.com`

Do **not** retry `listing@bybit.com` from the same Gmail account repeatedly (can flag your sender).

## Next actions

- [x] All four listing emails sent (Sep 14 2026)
- [x] Production stack running (`scripts/integrator_stack.ps1`) — gate 2/2, custody `:8787`, demo `:8788`
- [x] Integrator keys generated (`scripts/integrator_gen_keys.py`) — handoff `.local/integrator_pilot_credentials.txt`
- [x] Watchlist registered on custody API
- [x] Jackpot round 0 drawn (tx `0xd9801183…b12f25`, round 1 live, ends `1789447062`)
- [x] Outbox webhook deliver — 10/10 credit events → `http://127.0.0.1:18320/kaspa-events` (outbox empty)
- [x] Evidence pack refreshed — `.local/evidence/evidence_20260914-194553.json` (148 Rust + 138 pytest OK)
- [ ] **Discord** — paste `discord_funding_followup.txt` in funding thread (updated Sep 14 PM)
- [ ] **Bybit** — LinkedIn / help form (`bybit_linkedin.txt`); do not resend listing@bybit.com
- [x] **Video** — `C:\Users\Admin2\IONOS HiDrive\Build Files\Kaspa Video\out\kaspa_dev_quickstart.mp4` (~9.7 MB, Sep 14 2026)
- [ ] **CEX day 5–7** (Sep 19–21) — `open_followup_drafts.ps1` if no reply
- [x] **Bonus API** — selftest, deposits/export, receiver/stats, webhooks/verify (`integrator_selftest.ps1`)
- [ ] **Pilot handoff** — `integrator_pilot_bundle.ps1` when replying; `GET /v1/pilot/summary`
- [ ] **Stack** — nodes were down Sep 15 AM; restart kaspad + `integrator_stack.ps1`

Operator checklist: `powershell -File scripts/cex_outreach/run_remaining.ps1`
