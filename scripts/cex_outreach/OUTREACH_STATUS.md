# CEX outreach status (Sep 14 2026)

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
- [x] Jackpot auto-draw scheduled (`scripts/galleon_jackpot_draw_when_ready.ps1`, round ends `1789427323`)
- [ ] Bybit LinkedIn / help form if no bounce receipt in Sent folder (`bybit_linkedin.txt`)
- [ ] Monitor Discord funding thread; reply with `discord_funding_followup.txt`
- [ ] Record 5-min integrator demo (`scripts/media/dev_video_script.txt`)
- [ ] Follow-up bump at day 5–7 (Sep 19–21 2026) if no reply — `followup_day5.txt` + `open_gmail_drafts.ps1`
- [ ] Outbox webhook deliver when deposit DB not locked: `POST /v1/outbox/deliver` → `http://127.0.0.1:18320/kaspa-events`

Operator checklist: `powershell -File scripts/cex_outreach/run_remaining.ps1`
