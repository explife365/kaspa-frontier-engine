TN10 dual-node gate green

**State (8 Sep 2026):**

- **Node 1** `ws://127.0.0.1:18210`: synced, gap 0, 0 IBD peers
- **Node 2** `ws://127.0.0.1:28210`: synced (loopback tunnel), same band, 0 IBD peers
- **Gate:** `--min-healthy 2` **green (2/2)**

**Rehearsal:**

- Dual `tn10-wrpc-live --resnapshot-only` on 18210+28210: live UTXOs, 2 owned nodes
- `scripts/smoke.ps1`: passed

**Watch:**

```powershell
powershell -File scripts/tn10_ibd_watch.ps1
powershell -File scripts/tn10_host02_tunnel.ps1
cargo run --release --bin tn10-node-health -- --url ws://127.0.0.1:18210 --url ws://127.0.0.1:28210 --min-healthy 2 --json
```

Toccata broadcast remains fail-closed until kaspa-python-sdk #78 ships in a published wheel.

Public integrator repo: https://github.com/explife365/kaspa-frontier-engine
