"""Probe Galleon / Kasplex ERC-20 (read-only). Does not mint USDC.

  python examples/galleon_erc20.py
  python examples/galleon_erc20.py --token 0x... --holder 0x...
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from erc20 import token_balance, token_meta  # noqa: E402
from galleon import (  # noqa: E402
    CIRCLE_USDC_ETHEREUM,
    CIRCLE_USDC_ON_GALLEON,
    GALLEON_CHAIN_ID,
    GALLEON_RPC,
    GALLEON_TEST_USDC,
    circle_usdc_on_chain,
)
from kaspa_env import load_kaspa_env  # noqa: E402


def main() -> None:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Read ERC-20 metadata on Galleon")
    parser.add_argument("--rpc", default=GALLEON_RPC)
    parser.add_argument("--token", default=GALLEON_TEST_USDC, help="token address (default: Galleon test USDC)")
    parser.add_argument("--holder", help="optional 0x address for balanceOf")
    args = parser.parse_args()
    meta = token_meta(args.rpc, args.token)
    print(f"chain   {GALLEON_CHAIN_ID}  rpc {args.rpc}")
    print(f"token   {meta['address']}")
    print(f"name    {meta['name']}")
    print(f"symbol  {meta['symbol']}")
    print(f"decimals {meta['decimals']}")
    print("circle  no (Igra test USDC is not Circle cash USDC)")
    print(f"circle ethereum USDC {CIRCLE_USDC_ETHEREUM}")
    print(f"circle on Galleon    {circle_usdc_on_chain(GALLEON_CHAIN_ID)} (want None)")
    print(f"circle listed here   {CIRCLE_USDC_ON_GALLEON}")
    same = meta["address"].lower() == CIRCLE_USDC_ETHEREUM.lower()
    print(f"same as Circle ETH   {same}")
    if args.holder:
        raw = token_balance(args.rpc, args.token, args.holder)
        scaled = raw / (10 ** int(meta["decimals"]))
        print(f"balance {scaled}  ({raw} raw)  holder {args.holder}")


if __name__ == "__main__":
    main()
