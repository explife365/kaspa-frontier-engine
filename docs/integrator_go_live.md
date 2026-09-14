# Integrator API go-live (TN10 CEX rehearsal)

Not consensus. Not production mainnet custody without independent audit.

## Stack

| Process | Command | Role |
|---------|---------|------|
| Gate | `tn10-node-health --dual --min-healthy 2` | Fail-closed before credit |
| Ingest | `tn10-wrpc-live <addrs> --dual --database .local/tn10-wrpc-live.sqlite` | Deposit journal |
| API | `tn10-integrator-api` | CEX HTTP integration |
| Receiver | `tn10-outbox-receiver` | Staging webhook dry-run |
| Deliver | `tn10-outbox deliver <url>` | Push credits to exchange |

Quick start:

```powershell
powershell -File scripts/integrator_go_live.ps1 --check
powershell -File scripts/integrator_go_live.ps1 --api-only
```

## Auth

Generate keys (writes `kaspa.env` + `.local/integrator_pilot_credentials.txt`):

```powershell
python scripts/integrator_gen_keys.py
python scripts/integrator_gen_keys.py --force   # rotate
```

Header: `X-Integrator-Key: your-long-random-key`

## CEX endpoints (`tn10-integrator-api`)

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health` | Liveness (no auth) |
| GET | `/openapi.json` | OpenAPI stub |
| GET | `/v1/gate` | N-of-M node health |
| GET | `/v1/deposits` | List deposits (`?address=&state=&limit=&offset=`) |
| GET | `/v1/deposits/{txid}/{vout}` | Single deposit |
| GET | `/v1/outbox` | Pending credit/reverse events |
| POST | `/v1/outbox/{id}/ack` | Ack delivered event |
| POST | `/v1/outbox/deliver` | `{"url":"https://..."}` webhook delivery |
| GET | `/v1/withdrawals` | List withdrawals |
| GET | `/v1/withdrawals/{txid}/{vout}` | Single withdrawal |
| GET | `/v1/return-address/{txid}?vout=0` | Deposit sender (#435) |
| GET | `/v1/fee-estimate/{txid}` | Fee estimate (#615 partial) |
| GET | `/v1/evidence/latest` | Latest evidence JSON |
| GET/POST | `/v1/watchlist` | Register deposit addresses |
| POST | `/v1/webhooks/test` | `{"url":"..."}` test payload + HMAC |

Custody routes return **503** when owned-node gate is red (`INTEGRATOR_REQUIRE_GATE=1`).

## Pilot week mapping

| Week | CEX action | Your deliverable |
|------|------------|------------------|
| 1 | Give TN10 deposit addresses | `POST /v1/watchlist` + run `tn10-wrpc-live` |
| 1 | Poll deposits | `GET /v1/deposits` + `GET /v1/outbox` |
| 2 | Staging webhook | `POST /v1/webhooks/test` then `POST /v1/outbox/deliver` |
| 3 | Mainnet cutover | `scripts/cex_production_runbook.md` on their nodes |

## What exchanges still provide

- Hot wallet keys and signing
- HD deposit address generation (optional — you watch via watchlist)
- Their confirmation policy number (map to `INTEGRATOR_CONFIRMATION_DAA`)

## Evidence

```powershell
powershell -File scripts/integrator_evidence_pack.ps1
curl -H "X-Integrator-Key: $KEY" http://127.0.0.1:8787/v1/evidence/latest
```
