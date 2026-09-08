# CEX production runbook (TN10 rehearsal → production)

Operator checklist. Not a listing path. Not consensus. Fail closed on any gate red.

## 1. Owned nodes (minimum 2)

| Role | Rehearsal | Production |
|------|-----------|------------|
| Node 1 | `ws://127.0.0.1:18210` local kaspad | Independent host, loopback-only wRPC sidecar |
| Node 2 | `ws://127.0.0.1:28210` tunneled replica | Second independent host, different provider/region |

Requirements per node:

- rusty-kaspa **v2.0.1+**, TN10 (`--testnet --netsuffix=10`)
- `--utxoindex`, `--rpclisten-json=default` bound to loopback only
- P2P open; JSON/gRPC **not** on public interfaces

Health gate (must pass before deposits, withdrawals, wRPC live, outbox delivery):

```powershell
cargo run --release --bin tn10-node-health -- --dual --min-healthy 2 --max-daa-lag 100 --json
powershell -File scripts/tn10_gate.ps1 -Json
```

Env: `TN10_MIN_HEALTHY=2`, `TN10_OWNED_NODE_URLS=ws://node1:18210,ws://node2:18210` (production URLs via sidecar).

## 2. Deposit path

1. Generate deposit addresses (HD or per-user); never reuse across users.
2. `tn10-deposits` or `tn10-wrpc-live` with `--dual --min-healthy 2`.
3. Credit only after **DAA depth N** (configure per risk policy).
4. Emit outbox events; deliver to core banking via webhook.

Webhook delivery:

- Loopback rehearsal: `--allow-cleartext-loopback` on receiver only.
- Production: **mTLS required** off loopback (`scripts/tn10_receiver_mtls_certs.ps1` for rehearsal PEM layout).
- Idempotency-Key: `credit:<txid>:<vout>` — receiver must atomically dedupe before 2xx.

## 3. Withdrawal path

1. `tn10-withdraw` with exact txid / vout / dest / amount — conflicting facts fail closed.
2. Confirm on DAA depth N before marking complete.
3. SQLite WAL + `synchronous=FULL` for restart safety.

## 4. Covenant / L1 tokens (optional)

Blocked until [kaspa-python-sdk#78](https://github.com/kaspanet/kaspa-python-sdk/pull/78) publishes.

Gate: `python scripts/tn10_sdk_gate.py --json` → `ready: true`.

Until native `getUtxosByCovenantId` on kaspad: use `tn10-covenant-rpc` sidecar or kascov with explicit verification flags.

## 5. Evidence and audits

- Run `powershell -File scripts/integrator_evidence_pack.ps1` after material changes.
- Artifacts under `.local/evidence/` (gitignored).
- Skynet gap notes: `scripts/certik_score_plan.md` — Foundation ops, not L1 consensus audit.

## 6. Do not

- Bind 18210 / 18320 on public IPs.
- Treat two ports on one VM as independent failure domains.
- Patch kaspad for Move/Utreexo/DAGKnight lore.
- Auto-acknowledge outbox events without operator or remote 2xx + idempotency store.

## Rehearsal entrypoint

```powershell
powershell -File scripts/tn10_rehearsal.ps1
powershell -File scripts/tn10_rehearsal.ps1 -Covenant
```

Public repo: https://github.com/explife365/kaspa-frontier-engine
