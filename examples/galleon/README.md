# Igra Galleon (Foundry)

EVM testnet **chain 38836**. Stables and ERC-20 live here, not on kaspad.

```bat
forge test
python examples/galleon_faucet.py --status
python examples/galleon_faucet.py --drip
python examples/galleon_erc20.py
```

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
