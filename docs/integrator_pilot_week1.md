# CEX pilot — week 1 deliverables

TN10 rehearsal. Not mainnet custody.

## Exchange provides

- Staging webhook URL (HTTPS)
- 1–10 TN10 deposit addresses to watch
- Confirmation policy (DAA depth — default 10 in API)

## We provide

| Deliverable | How |
|-------------|-----|
| Gate status | `GET /v1/gate` |
| Pilot snapshot | `GET /v1/pilot/summary` |
| Deposit journal | `GET /v1/deposits` |
| Watchlist registration | `POST /v1/watchlist` |
| Webhook test | `POST /v1/webhooks/test` |
| Evidence JSON | `GET /v1/evidence/latest` or evidence pack script |
| Handoff zip | `powershell -File scripts/integrator_pilot_bundle.ps1` |

## Operator prep

```powershell
powershell -File scripts/tn10_kaspad.ps1          # node 1
powershell -File scripts/tn10_host02_tunnel.ps1     # node 2
powershell -File scripts/integrator_stack.ps1
powershell -File scripts/cex_outreach/check_channels.ps1
powershell -File scripts/integrator_pilot_bundle.ps1
```

## Week 2 preview

- `POST /v1/outbox/deliver` to staging URL
- Withdrawal registration via `tn10-withdraw` CLI
- Idempotency-Key contract: `credit:<txid>:<vout>`
