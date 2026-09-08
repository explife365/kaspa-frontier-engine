TN10 dual-node gate green

**State (8 Sep 2026 ~09:22 EDT):**

- **Laptop** `ws://127.0.0.1:18210`: synced, DAA ~565.15M, gap 0, 9 peers, 0 IBD
- **Replica** `ws://127.0.0.1:28210`: synced via host02 tunnel, same band, 9 peers, 0 IBD
- **Gate:** `--min-healthy 2` **green (2/2)**
- **host02 disk:** purged `/var/lib/tuce_ops/mamajama_runs` (~449G); `/` now ~249G/698G used (~449G free)

**Rehearsal:**

- Dual `tn10-wrpc-live --resnapshot-only` on 18210+28210: 5 live UTXOs, 2 owned nodes
- `scripts/smoke.ps1`: passed

**Watch:**

```powershell
powershell -File scripts/tn10_ibd_watch.ps1
powershell -File scripts/tn10_host02_tunnel.ps1
cargo run --release --bin tn10-node-health -- --url ws://127.0.0.1:18210 --url ws://127.0.0.1:28210 --min-healthy 2 --json
```

Toccata broadcast remains fail-closed until kaspa-python-sdk #78 ships in a published wheel.
