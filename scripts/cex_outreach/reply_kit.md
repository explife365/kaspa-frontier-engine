# CEX reply kit (TN10 integrator pilot)

Use when Gate, MEXC, KuCoin, or Bybit respond to the initial outreach.

## Attach / link

| Artifact | Location |
|----------|----------|
| Evidence gist | https://gist.github.com/explife365/477afea386ddba43574c7cb841ad4c73 |
| Repo | https://github.com/explife365/kaspa-frontier-engine |
| Demo video | `C:\Users\Admin2\IONOS HiDrive\Build Files\Kaspa Video\out\kaspa_dev_quickstart.mp4` |
| OpenAPI | `docs/integrator_openapi.yaml` (or live `GET /openapi.json`) |
| Runbook | `docs/integrator_go_live.md` + `scripts/cex_production_runbook.md` |

## One-paragraph pilot SOW

TN10 rehearsal only — not mainnet custody. We provide:

1. **Owned-node gate** — N-of-M independent kaspad nodes; API returns 503 when gate is red.
2. **Deposit journal** — wRPC ingest, DAA confirmation depth configurable, SQLite WAL.
3. **Outbox webhooks** — idempotent `credit:<txid>:<vout>` events with optional HMAC (`X-Integrator-Signature`).
4. **Withdrawal tracking** — register expected outpoint, observe spend, confirm on DAA depth.
5. **Return-address + fee estimate** — deposit sender resolution (#435) and fee partial (#615).

Exchange provides: hot wallet keys, deposit address generation, staging webhook URL, confirmation policy.

## Staging handoff (week 1)

```text
API base:     https://<your-staging-host>/  (mTLS in production)
Auth header:  X-Integrator-Key: <pilot-key>
Gate check:   GET /v1/gate
Watchlist:    POST /v1/watchlist  {"addresses":["kaspatest:..."]}
Poll:         GET /v1/deposits?state=credited
Outbox:       GET /v1/outbox
Webhook test: POST /v1/webhooks/test  {"url":"https://your-staging/callback"}
```

## Evidence refresh before call

```powershell
powershell -File scripts/integrator_evidence_pack.ps1
curl -H "X-Integrator-Key: $KEY" http://127.0.0.1:8787/v1/evidence/latest
```

## Boundaries (say explicitly)

- Not a listing commitment or liquidity provision.
- Not consensus; TN10 only until independent audit on mainnet path.
- Do not patch kaspad BPS or ship unmerged rusty-kaspa changes as production.
- Two ports on one VM ≠ two failure domains for production gate.

## Follow-up timing

- Day 5–7: `scripts/cex_outreach/followup_day5.txt`
- Bybit bounce: `scripts/cex_outreach/bybit_linkedin.txt`
