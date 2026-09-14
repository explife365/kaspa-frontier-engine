# Build on Kaspa faster — developer paths (text)

Testnet rehearsal only. Not consensus. Fail-closed on anything not shipped.

## Pick your path (15 minutes)

| You have | Start here | Ship what |
|----------|------------|-----------|
| No disk, no keys | `python examples/dev_quickstart.py --path rest` | Parity quest + offline proofs |
| Faucet only | `python examples/dev_quickstart.py --path galleon` | Galleon games + pool quotes |
| ~450G + time | `python examples/dev_quickstart.py --path nodes` | Owned-node gate + deposits |
| Wallet + SDK patch | `python examples/dev_quickstart.py --path covenant` | SilverScript covenant tx |

## Blockers → shims (swap later, no rewrite)

| Blocker | Workaround now | Command |
|---------|----------------|---------|
| No `getUtxosByCovenantId` | kascov + `tn10-covenant-rpc` | `python scripts/integrator_shims.py --json` |
| SDK wheel pending | `TN10_SDK_DEV_PATCH=1` | `python scripts/tn10_sdk_gate.py --json` |
| No owned node | Public REST checklist | `python scripts/tn10_adoption_scorecard.py --public-only` |
| No L1 EVM | Galleon L2 (gTEST) | `python examples/galleon_games.py list` |

When rusty-kaspa#1128 or kaspa-python-sdk#78 lands, the same `integrator_shims.py` routes flip to native — see `swap_when` in JSON.

## One-command health

```bash
python examples/integrator_status.py --skip-gate
python examples/integrator_api.py --port 8787
# browser: examples/kaspa_frontier_dashboard.html
```

## Media (share with team)

- **Video script:** `scripts/media/dev_video_script.txt` (~3 min screencast)
- **Audio brief:** `scripts/media/dev_audio_script.txt` (~90 s voice note)
- **Visual canvas:** open `kaspa-frontier-leader` in Cursor

## Why Kaspa for apps

- **10 BPS GHOSTDAG** — parallel blocks, not one tip
- **Toccata covenants on L1** — logic without EVM on kaspad
- **L2 (Galleon)** — ERC-20 / games / AMM rehearsal
- **Sub-second target cadence** — UX that feels like an app, not a museum

## Next after hello-world

1. Offline verify: `cargo run --release --bin tn10-proof -- fixtures/tn10-counter-proof.json --offline`
2. Play: `python examples/galleon_games.py parity --rounds 3`
3. Post status: `python examples/integrator_status.py --json`
4. Node path: `powershell -File scripts/tn10_node_onboard.ps1`

Repo: https://github.com/explife365/kaspa-frontier-engine
