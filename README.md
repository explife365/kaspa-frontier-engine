# kaspa-frontier-engine

Honest testnet-10 crate extracted from the Gemini “Path to Top Ten” chat.

Gemini’s listing/DeFi/PoW **strategy** is real. Most of the Rust it produced is **not** rusty-kaspa: simulated workers (`wrapping_mul(42)`), fake file paths under `consensus/`, invented Criterion numbers, and a ZK verifier that always returns `true`.

This repo keeps the parts that can be made true.

Independent developer of this crate. Optional mainnet KAS (not the official Kaspa Dev Fund):

`kaspa:qpxdemlyx445kt5xteux0qhadaw8lh5m0vnqvcy8fh483t70usgkkeulsx9cm`

https://explorer.kaspa.org/addresses/kaspa:qpxdemlyx445kt5xteux0qhadaw8lh5m0vnqvcy8fh483t70usgkkeulsx9cm

## What this is

| Module | Role |
| --- | --- |
| `tn10-status` | Live TN10 DAG, REST hashrate, fee estimate, Kasplex indexer, Igra/Kasplex L2 probes |
| `tn10-deposits` | Rehearsal watcher with transactional SQLite state/outbox; confirms on DAA |
| `tn10-withdraw` | Durable REST rehearsal: exact txid/dest/vout/amount must reach N DAA |
| `tn10-proof` | Checks proof txids, Toccata v1 fields, and kascov covenant lineage on TN10 |
| `exchange` | Deposit tracker **and** outbound DAA confirmation (submit ≠ done) |
| `tn10-kasplex` | Live Kasplex tokenlist / address balances / open mints (inscriptions, not USD) |
| `krc20` | Off-chain KRC-20 state machine **and** canonical Kasplex inscription JSON |
| `covenant` | Timelock + destination policy. Refuses stub ZK |
| `telemetry` | GHOSTDAG metrics from live difficulty. Does not claim DAGKnight |

## Connect to testnet-10

You do **not** need a local node for `tn10-status`.

This Windows box has no MSVC `link.exe`. The repo pins `stable-x86_64-pc-windows-gnu` in `rust-toolchain.toml`. Keep WinLibs `mingw64\\bin` on PATH (gcc).

```bat
python -m pip install -r requirements.txt
cargo test
cargo test -- --ignored
cargo run --release --bin tn10-status
cargo run --release --bin tn10-deposits -- kaspatest:<addr> --ledger .local/tn10-deposits.sqlite
cargo run --release --bin tn10-withdraw -- kaspatest:<dest> <txid> <vout> <amount-sompi> 60 --database .local/tn10-withdrawals.sqlite
cargo run --release --bin tn10-proof
cargo run --release --bin tn10-covenant-rpc
python examples/silverscript/counter.py --print-address
python examples/silverscript/counter.py
python examples/tn10_transfer.py --print-wallets
python examples/tn10_transfer.py --topup
python examples/galleon_faucet.py --drip
cargo run --release --bin tn10-kasplex
python examples/kasplex_krc20.py
python examples/kasplex_krc20.py --commit-address --tick TMBMN
python examples/kasplex_krc20.py --mint TMBMN --from alice
python examples/kasplex_krc20.py --distribute --from alice --amt 10000000000
python examples/kasplex_krc20.py --deploy FRNTR
python -m unittest discover -s tests -p "test_*.py"
```

Reproducible offline gate (pinned Rust 1.80.1, locked Cargo graph, no network or broadcasts):

```powershell
powershell -ExecutionPolicy Bypass -File scripts/smoke.ps1
```

Offline wRPC v2 replay rehearsal (strict node notification envelopes, append-only SQLite
journal, gap-free checkpoints, and idempotent deposit reconciliation):

```powershell
cargo run --bin tn10-wrpc-replay
```

This command reads `fixtures/wrpc-utxos-replay.jsonl` and performs no network calls. The
owned-node transport is also available; start a synced TN10 node with `--utxoindex` and
`--rpclisten-json=default`, verify its REST resnapshot first, then subscribe:

```powershell
cargo run --bin tn10-wrpc-live -- kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt --resnapshot-only
cargo run --bin tn10-wrpc-live -- kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt
cargo run --bin tn10-wrpc-live -- kaspatest:<deposit-1> kaspatest:<deposit-2> --database .local/tn10-custody.sqlite
```

Cleartext wRPC is restricted to loopback. Startup, reconnect, and periodic recovery perform
a bounded 1–100 address TN10 REST resnapshot; every notification is validated against the
subscribed address set, journaled before ledger application, and
applied journal rows are compacted while retaining a replay tail. Steady-state DAA frames
use a maturity schedule instead of scanning every live UTXO; frames with no ledger delta are
appended and checkpointed in one FULL-synchronous transaction. Production custody still
needs supervised node operations and a downstream consumer that enforces the supplied
idempotency key.

Use the owned-node health gate in service readiness checks. It exits nonzero unless the
loopback node is TN10, v2.0.1+, synchronized, running `--utxoindex`, internally consistent,
and no more than 100 DAA behind the public TN10 snapshot:

```powershell
cargo run --release --bin tn10-node-health
cargo run --release --bin tn10-node-health -- --json --max-daa-lag 100
```

The deposit outbox never auto-acknowledges. Inspect it, explicitly acknowledge a manually
handled event, or deliver leased events to an HTTPS webhook:

```powershell
cargo run --bin tn10-outbox -- list
cargo run --bin tn10-outbox -- ack 1
cargo run --bin tn10-outbox -- deliver https://custody.example/kaspa-events --limit 100
```

For the REST polling database, add `--database .local/tn10-deposits.sqlite`. Webhook requests
include `Idempotency-Key: credit:<txid>:<vout>` (or `reverse:...`) and a versioned JSON body.
The receiver must atomically store that key before returning 2xx. Delivery is at least once:
a crash after remote success but before local acknowledgement intentionally retries the same
key. SQLite leases prevent concurrent local workers from sending the same event; expired
leases are reclaimable. Existing schema-v1 ledgers migrate transactionally to schema v2.

Withdrawal expectations and the first exact destination-output observation are also stored
under SQLite WAL with `synchronous=FULL`. A restart can therefore confirm an accepted
withdrawal from its durable block DAA even after the recipient spends the output. Conflicting
txid/vout/address/amount/block-DAA facts and accepted-to-rejected transitions fail closed.

`counter.py --print-address` remains safe, but transaction funding/broadcast currently fails closed: the pinned Python SDK drops the Toccata v1 `computeBudget` field during serialization. Resume covenant broadcasts only after installing a build containing [rusty-kaspa PR #1074](https://github.com/kaspanet/rusty-kaspa/pull/1074) and updating the conformance test.

Optional local node (rusty-kaspa **v2.0.1** Toccata, wRPC JSON on 18210). Binaries live in `%LOCALAPPDATA%\kaspa\v2.0.1` (on user PATH):

```bat
kaspad --testnet --netsuffix=10 --utxoindex --rpclisten-json=default
```

Galleon ERC-20 (Igra L2 **38836**, Foundry **v1.7.1** + solc **0.8.24**). Binaries live in `%USERPROFILE%\.foundry\bin`:

```bat
cd examples\galleon
forge test
cast chain-id --rpc-url galleon_testnet
```

Galleon primary `0xb39f360A…` is live. Igra still silently drops a tx if `balance < value + gasLimit × gasPrice`. Minimum observed relay gas price is **2000 gwei**. `HelloIgra` is at `0x6311651bE0BcE57c81516d140d4c0Fd708472048`. `GalleonIkasFaucet` is at `0x0510267c5dBad52c6c9A9cA0dCB45DAB9203035d` (owner-push `dripTo` only). **`gTEST` is live** at `0xbc5e27ab3ce2edb243593cda2437e5b30e0d5d7d` (permit ERC-20, 18 decimals, not USD, not Circle). **`wiKAS` is live** at `0x7331b0a33ac9aa92f506f057bfaa049ea133f77f` (WETH9-style wrap; not kaspad; not USD). The official faucet is a shared test resource; use one wallet, respect its daily limits, and retry later when capped.

```bat
python examples/galleon_faucet.py --status
python examples/galleon_faucet.py --drip
python examples/galleon_faucet.py --send 0xDEST --ikas 0.01
cd examples\galleon
forge test
```

`gTEST` is a Galleon test ERC-20 (not USD) at `0xbc5e27ab3ce2edb243593cda2437e5b30e0d5d7d`. `wiKAS` is live at `0x7331b0a33ac9aa92f506f057bfaa049ea133f77f`. This crate cannot mint iKAS.

**Test gas:** Igra’s faucet is 0.1 iKAS per address per UTC day and may enforce a connection cap. Do not automate IP rotation or multi-account limit bypasses. If capped, retry later. If you already hold TN10 tKAS, use an **official** grind UI only (Kasperia / ikas.katbridge.com); the L1 Entry txid must start with `97b4`. This crate encodes the payload and does not grind. Do not send tKAS without that prefix. `GalleonIkasFaucet` is unfunded and cannot mint.

- Address prefix: `kaspatest:`
- Faucet: https://faucet-tn10.kaspanet.io/
- Do not use a mainnet seed.

Secrets: copy `kaspa.env.example` → `kaspa.env` (gitignored). Never commit keys. Proof JSON may still live under `.local/`. Named wallets: alice, bob, carol, dave, eve (`python examples/tn10_transfer.py --print-wallets`).

TN11 is defunct. Gemini’s `grpc://127.0.0.1:16110` is **mainnet** gRPC.

## What the community is asking for

From kaspa.org’s integrator call, the Toccata guide, and kascov (not Discord — this box has no Discord login):

| Ask | Who | This crate |
| --- | --- | --- |
| Run a TN10 node and test deposits / withdrawals / indexing / tx parsing | Core, pools, exchanges | `tn10-deposits` + restart-safe `tn10-withdraw` (REST DAA). The owned kaspad v2.0.1 is synced with `--utxoindex`; `tn10-node-health` provides a fail-closed readiness gate. We do not ship kaspad. |
| Parse v1 txs: `storageMass`, `compute_budget`, output `covenant_id` | rusty-kaspa Toccata guide | `tn10-proof` requires exact selected-output lineage and the complete previous outpoint of each covenant-authorizing input |
| Fee estimation rehearsal | kaspa.org integrator call | `tn10-status` and `tn10_transfer.py` print `/info/fee-estimate` (minimum standard mempool/RPC policy, not consensus) |
| Wallet / explorer covenant decode (UX lag) | Core R&D | Toccata is live (~517 mainnet covenants vs ~80k TN10). We print lineage; Covex / [kascov](https://kascov.io/) are the UIs. |
| `getUtxosByCovenantId` on the node | Missing in kaspad | Community indexer: `https://kascov.io/data/testnet-10/c/<id>.json`. Local bounded Axum shim: `tn10-covenant-rpc` (REST+kascov, loopback by default, unverified community UTXOs, **not** kaspad). |
| Silverscript apps with public TN10 txids | Toccata docs | `examples/silverscript/counter.py`; checked-in offline evidence under `fixtures/`, with `cargo run --release --bin tn10-proof` as a read-only live smoke test |
| EVM stables / Uniswap | Igra / Kasplex L2, **not** L1 | L1 has no EVM (UTXO + Toccata covenants). Solidity lives on Galleon `38836` / Kasplex L2 `167012`. `gTEST` `0xbc5e27ab…5d7d` is on Galleon (not USD). Circle USDC is not listed. Uniswap cannot be created on kaspad. |
| KRC-20 commit/reveal vs `tn10api.kasplex.org` | Kasplex | `tn10-kasplex` + `examples/kasplex_krc20.py`. Frontier tick **TMBMN** is live (mint+transfer). Not USD. `--deploy` of a crate-owned tick burns **1000 tKAS**. |
| DAGKnight / 100 BPS lore | Narrative only | KIP-2 Proposed. Live is GHOSTDAG @ 10 BPS. Fake telemetry does not activate it. Refused. |
| Archival / indexer cost | Exchanges / ops | Need `getUtxosByAddresses` + DAA depth, not a simulated worker. `cex::snapshot_address` + `tn10-deposits` / `tn10-withdraw`. |
| CEX integration rehearsal | Integrator call / `Kaspa to do.pdf` | Partial: bounded multi-address snapshots/ingestion, durable exact withdrawals, leased idempotency-key webhook outbox, delta-driven durable wRPC replay, owned-node reconnect resnapshots, and a supervisor health gate. Production custody still requires redundant node operations and a receiver that atomically deduplicates delivery keys. |

Do **not** open unofficial consensus PRs against rusty-kaspa. Acceptable PRs there follow their review process and KIPs.

The community accepts:

- Real `kaspatest:` transactions, public explorer txids, wallet/explorer UX
- Tools that talk to kaspad / the public REST API without claiming to be the node
- KIPs and PRs to [rusty-kaspa](https://github.com/kaspanet/rusty-kaspa) that follow their review process

The community will **not** accept:

- Unofficial consensus patches (Move VM, Utreexo, STARK stubs, DAGKnight-as-live)
- Mainnet protocol changes without a KIP
- Treating this repo, Igra L2, or Kasplex as Core R&D

Roadmap this crate follows: Toccata live, SilverScript experimental on TN10, KCC-0020 draft, DAGKnight research only. Live KRC-20 is Kasplex; the in-memory engine is only a payload checker.

## What is blocking kaspad

kaspad is not waiting on this crate. Live L1 is UTXO + GHOSTDAG @ 10 BPS + Toccata covenants. These community asks **cannot** land in kaspad without a KIP (and some never will):

| Ask | Why it is not on L1 | Where it actually lives |
| --- | --- | --- |
| Solidity / USDC / Uniswap | No EVM runtime | Igra Galleon / Kasplex L2. Galleon test USDC `0xFd89676CBb3D2742c565aFC02986370ef4ba667A` is Igra test, not Circle. |
| `getUtxosByCovenantId` | Missing from kaspad RPC | [kascov](https://kascov.io/) indexer; `tn10-proof` already uses it |
| Native covenant tokens | KCC-0020 is draft | Not Kasplex KRC-20; wait for ratification |
| DAGKnight / 100 BPS lore | KIP-2 Proposed. Fake telemetry does not activate it | Research. Live protocol is GHOSTDAG @ 10 BPS |
| Covenant UX lag | Consensus already accepts Toccata v1 | Wallets/explorers catching up (~517 mainnet vs ~80k TN10) |
| Archival / indexer cost | Ops, not missing consensus | `getUtxosByAddresses` + DAA depth via REST; `cex::snapshot_address` is the CEX wrapper. Do not port `Kaspa to do.pdf`’s account StateDB. |
| Binance/Coinbase spot | Exchange custody + demand | The CEX. This crate only rehearses deposit/withdraw DAA |

`tn10-status` prints **what is blocking kaspad**, integrator next, and this crate’s 14-row community-ask board (tally of shipped/partial/l2/refused). Do not patch rusty-kaspa to fake any of it.

## What will not put Kaspa in the top 10

- A local deposit listener does not list KAS on Binance/Coinbase. BTC listed because **those exchanges** custody it and wanted a BTC pair — Satoshi never incorporated. KAS can list the same demand-pull way (Kraken/MEXC already did). This crate is not an exchange.
- L1 has no EVM: kaspad does not run Solidity/ERC-20/Uniswap. Those contracts belong on Igra or Kasplex L2. KRC-20 is an inscription indexer, not USDT.
- An in-memory KRC-20 map does not create USDT TVL.
- Fake 22k TPS / 1.8s IBD numbers do not change consensus.

Adoption work that *does* matter: a TN10-proven covenant or Kasplex flow, a wallet that signs it, a public txid.

Live Kasplex Frontier token (tick TMBMN, `opAccept=1`). Crate TN10 KRC-20, not USD:

- commit https://explorer-tn10.kaspa.org/txs/1a3a634b8a62eae0e366a04eaae98735b03be3b416dab3102e4b445f7004d6fc
- reveal https://explorer-tn10.kaspa.org/txs/d3ae52c2063ad0c3731fe32a6dc6f50d9d7b767d3b0da17778146d29a9f87c36
- indexer https://tn10api.kasplex.org/v1/krc20/op/d3ae52c2063ad0c3731fe32a6dc6f50d9d7b767d3b0da17778146d29a9f87c36

Live Kasplex transfer (alice → bob, 50000000000 TMBMN, `opAccept=1`). Frontier, not USD:

- commit https://explorer-tn10.kaspa.org/txs/a413b350db5cb65fd168137b7f3bd828eee70399cf632e9dfa8ec6f57218d779
- reveal https://explorer-tn10.kaspa.org/txs/f789eb6474377056302b0996ee67c9a12fb9d92e17042b9b5b4b508b66299316
- indexer https://tn10api.kasplex.org/v1/krc20/op/f789eb6474377056302b0996ee67c9a12fb9d92e17042b9b5b4b508b66299316

## Gemini items discarded on purpose

- rusty-kaspa patches (`utreexo/accumulator.rs`, `move_executor.rs`, `stark_verifier.rs`) — those files are not a real integration map
- Discord bot + SIMD pipeline claiming to be the node
- DAGKnight dynamic 100 BPS telemetry
- `verify_zk_groth16` → `true`
