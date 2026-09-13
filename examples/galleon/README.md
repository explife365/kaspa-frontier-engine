# Igra Galleon (Foundry)

EVM testnet **chain 38836**. Stables and ERC-20 live here, not on kaspad.

```bat
forge test
python examples/galleon_faucet.py --status
python examples/galleon_faucet.py --drip
python examples/galleon_erc20.py
python examples/galleon_pool.py --status
python examples/galleon_pool.py --swap 0.1 --zero-for-one --dry-run
python examples/galleon_pool.py --swap 0.1 --zero-for-one --broadcast
python examples/galleon_pool_seed.py --broadcast
python examples/l1_l2_bridge_deploy.py --simulate
python examples/l1_l2_bridge_deploy.py --broadcast
python examples/l1_l2_bridge_release.py --status
python examples/l1_l2_bridge_release.py --fund --broadcast
python examples/l1_l2_bridge_release.py --claim --broadcast
python examples/l1_l2_bridge_deploy_sha256.py --simulate
python examples/l1_l2_bridge_deploy_sha256.py --broadcast
python examples/l1_l2_bridge_release_sha256.py --fund --broadcast
python examples/l1_l2_bridge_release_sha256.py --claim --broadcast
python examples/l1_l2_bridge_finish_sha256.py --status
python examples/l1_l2_bridge_finish_sha256.py --broadcast
python examples/l1_l2_htlc_bridge.py
```

Live pool (gTEST / wiKAS): `0xa423c4f6930e0bdb2fa32470441767dff3937d3d`

**GalleonMiniPool** (`src/GalleonMiniPool.sol`) is a minimal constant-product rehearsal AMM.
Forge tests cover reserves, quotes, and swaps. Deploy with `forge script` (constructor takes
two ERC-20 addresses), then probe live reserves:

```bat
python examples/galleon_pool.py --pool 0xDEPLOYED --token0 0xbc5e27ab3ce2edb243593cda2437e5b30e0d5d7d --token1 0x7331b0a33ac9aa92f506f057bfaa049ea133f77f --status
python examples/galleon_pool.py --pool 0xDEPLOYED --quote 1.0 --zero-for-one
```

Not Uniswap. Not production liquidity. Rehearsal only on Galleon testnet.

### Cash-flow rehearsal (Sep 2026)

Three parallel revenue paths — all testnet, not consensus:

| Product | Contract | Fee | Python |
|---------|----------|-----|--------|
| Fee AMM | `GalleonFeePool.sol` (30 bps; 50% to treasury) | swap fees | `galleon_fee_pool_deploy.py`, `galleon_fee_pool.py` |
| Bridge relayer | `HtlcBridgeFactory.sol` + `HtlcBridgeVaultSha256.sol` | 1% on claim | `galleon_bridge_factory_deploy.py`, `galleon_bridge_relayer.py` |
| Integrator API | n/a (hosted status) | SaaS rehearsal | `integrator_api.py` (`GET /v1/status`, `/v1/adoption`, `/v1/fixtures`) |

```bat
forge test --match-contract GalleonFeePoolTest
forge test --match-contract HtlcBridgeFactoryTest
python examples/galleon_fee_pool_deploy.py --simulate
python examples/galleon_bridge_factory_deploy.py --simulate
python examples/integrator_api.py --port 8787
```

### Galleon DEX (gTEST / wiKAS)

Unified swap hub on the live MiniPool (or FeePool after deploy):

```bat
python examples/galleon_dex.py status
python examples/galleon_dex.py quote --sell 1.0 --buy wiKAS
python examples/galleon_dex.py swap --sell 0.5 --buy wiKAS --dry-run
python examples/galleon_dex.py swap --sell 0.5 --buy wiKAS --broadcast
python examples/galleon_dex.py serve
```

Open `examples/galleon_dex.html` while `serve` runs (quotes via `GET /v1/dex/quote`).

Deploy fee-tier pool (30 bps, LP + treasury):

```bat
python examples/galleon_dex.py deploy-fee
python examples/galleon_fee_pool.py --status
```

### DeFi games (community fun)

| Game | Contract | CLI |
|------|----------|-----|
| Coin flip | `GalleonCoinFlip.sol` | `galleon_coin_flip.py` |
| Dice | `GalleonDice.sol` | `galleon_dice.py` |
| Jackpot | `GalleonJackpot.sol` | `galleon_jackpot.py` |
| Parity quest | TN10 REST (no gas) | `parity_quest.py` |

```bat
forge test --match-contract GalleonGamesTest
python examples/galleon_games.py list
python examples/galleon_games_deploy.py --simulate
```

Demo randomness only — not production gambling.

The official faucet is a shared test resource. Use one wallet, respect daily
limits, and do not automate IP rotation or multi-account limit bypasses.

Igra silently drops a tx if `balance < value + gasLimit * gasPrice`. Floor is
**2000 gwei**. Native 21_000-gas sends fit. Contract creates need the full
prepaid amount in the wallet first (`gTEST` simulate was ~1.40 iKAS, live; `wiKAS` simulate is **1.507308** iKAS at 753654 gas × 2000 gwei).

On-chain:

- `HelloIgra` `0x6311651bE0BcE57c81516d140d4c0Fd708472048` — use `forge script`, not
  `forge create` with a constructor string (Foundry splits the string into extra ABI args).
- `GalleonIkasFaucet` `0x0510267c5dBad52c6c9A9cA0dCB45DAB9203035d` — owner-push only.
- `gTEST` `0xbc5e27ab3ce2edb243593cda2437e5b30e0d5d7d` — permit ERC-20. Not USD. Not Circle.
- `wiKAS` `0x7331b0a33ac9aa92f506f057bfaa049ea133f77f` — WETH9-style wrapper. Not kaspad. Not Circle USDC.

```bat
python examples/galleon_faucet.py --send 0xDEST --ikas 0.01
python examples/galleon_erc20.py --token 0xbc5e27ab3ce2edb243593cda2437e5b30e0d5d7d
```

gTEST `permit` is the USDC-style gasless approve (relayer pays). Not USD. Live on Galleon.

RPC: `https://galleon-testnet.igralabs.com:8545`
Explorer: https://explorer.galleon-testnet.igralabs.com
