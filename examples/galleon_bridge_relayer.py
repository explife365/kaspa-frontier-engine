"""Galleon HTLC bridge relayer rehearsal: per-order vaults with claim spread.

  python examples/galleon_bridge_relayer.py --status
  python examples/galleon_bridge_relayer.py --quote --payout 0.001 --recipient 0x...
  python examples/galleon_bridge_relayer.py --create-vault --payment-hash 0x... --payout 0.001 --recipient 0x... --dry-run
  python examples/galleon_bridge_relayer.py --create-vault ... --broadcast

Requires GALLEON_BRIDGE_FACTORY after galleon_bridge_factory_deploy.py --broadcast.
Not production bridge ops. TN10 L1 claim is separate (l1_l2_bridge_release_sha256.py).
"""

from __future__ import annotations

import argparse
import os
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from eth_utils import keccak

from erc20 import decode_uint256, eth_call  # noqa: E402
from galleon import GALLEON_BRIDGE_FACTORY, GALLEON_CHAIN_ID, GALLEON_RPC, GALLEON_WRAPPED_IKAS  # noqa: E402
from galleon_faucet import galleon_key, require_galleon_chain  # noqa: E402
from galleon_pool_seed import send_contract  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402

SELECTOR_CLAIM_FEE_BPS = "0x" + keccak(text="claimFeeBps()").hex()[:8]
SELECTOR_CREATE = "0x" + keccak(text="createVault(bytes32,uint256,address,uint256)").hex()[:8]
SELECTOR_QUOTE_CLAIM = "0x" + keccak(text="quoteClaim()").hex()[:8]
DEFAULT_DEADLINE_SEC = 7 * 24 * 3600
CREATE_GAS = 900_000


def factory_address() -> str:
    addr = (os.environ.get("GALLEON_BRIDGE_FACTORY") or GALLEON_BRIDGE_FACTORY or "").strip()
    if not addr:
        raise RuntimeError("missing GALLEON_BRIDGE_FACTORY; run galleon_bridge_factory_deploy.py --broadcast")
    return addr


def parse_amount(text: str, decimals: int = 18) -> int:
    whole, frac = (text.split(".", 1) + ["0"])[:2]
    frac = (frac + "0" * decimals)[:decimals]
    return int(whole) * (10**decimals) + int(frac or "0")


def encode_create_vault(payment_hash: str, payout: int, recipient: str, deadline: int) -> str:
    ph = payment_hash.lower().removeprefix("0x").zfill(64)
    pr = recipient.lower().removeprefix("0x").zfill(64)
    return SELECTOR_CREATE + ph + f"{payout:064x}" + pr + f"{deadline:064x}"


def read_claim_fee_bps(rpc: str, factory: str) -> int:
    return decode_uint256(eth_call(rpc, factory, SELECTOR_CLAIM_FEE_BPS))


def quote_claim(rpc: str, vault: str) -> tuple[int, int]:
    raw = eth_call(rpc, vault, SELECTOR_QUOTE_CLAIM)
    body = raw.removeprefix("0x")
    return decode_uint256("0x" + body[:64]), decode_uint256("0x" + body[64:128])


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Galleon bridge relayer rehearsal")
    parser.add_argument("--rpc", default=GALLEON_RPC)
    parser.add_argument("--factory", default="")
    parser.add_argument("--status", action="store_true")
    parser.add_argument("--quote", action="store_true")
    parser.add_argument("--payout", default="0.001")
    parser.add_argument("--recipient", default="")
    parser.add_argument("--payment-hash", default="")
    parser.add_argument("--vault", default="", help="existing vault for quoteClaim")
    parser.add_argument("--create-vault", action="store_true")
    parser.add_argument("--deadline-sec", type=int, default=DEFAULT_DEADLINE_SEC)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    args = parser.parse_args()

    require_galleon_chain()
    factory = args.factory or factory_address()
    fee_bps = read_claim_fee_bps(args.rpc, factory)
    print(f"chain   {GALLEON_CHAIN_ID}")
    print(f"factory {factory}")
    print(f"claim fee {fee_bps} bps ({fee_bps / 100:.2f}%)")
    print("not production bridge — Galleon testnet rehearsal")

    if args.quote or args.status:
        payout = parse_amount(args.payout)
        fee = payout * fee_bps // 10_000
        net = payout - fee
        print(f"quote payout {args.payout} wiKAS -> recipient {net / 1e18:.6f}  treasury fee {fee / 1e18:.6f}")

    if args.vault:
        net, fee = quote_claim(args.rpc, args.vault)
        print(f"vault {args.vault}  net={net}  fee={fee}")

    if args.create_vault:
        if not args.payment_hash or not args.recipient:
            raise SystemExit("--create-vault needs --payment-hash and --recipient")
        payout = parse_amount(args.payout)
        deadline = int(time.time()) + args.deadline_sec
        data = encode_create_vault(args.payment_hash, payout, args.recipient, deadline)
        print(f"createVault payout={payout} recipient={args.recipient} deadline={deadline}")
        if args.broadcast:
            key = galleon_key()
            send_contract(key, factory, data, CREATE_GAS, 0)
            print("vault deploy tx sent — parse logs for address")
        else:
            print("dry-run createVault")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
