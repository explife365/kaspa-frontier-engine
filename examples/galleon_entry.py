"""Lock TN10 tKAS via Galleon L1 Entry (txid prefix 97b4) to mint iKAS on Galleon.

  python examples/galleon_entry.py --dry-run --wallet bob --kas 1
  python examples/galleon_entry.py --wallet dave --kas 1 --broadcast

Requires >= 1 tKAS spendable on the named TN10 wallet. Grinds payload nonce until
the signed txid starts with 97b4 (Igra requirement). Does not use the faucet.
"""

from __future__ import annotations

import argparse
import asyncio
import os
import sys
import time
from pathlib import Path

from kaspa import Address, PaymentOutput, Resolver, RpcClient, create_transactions, kaspa_to_sompi

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "examples"))
from galleon import (  # noqa: E402
    GALLEON_ENTRY_ADDRESS,
    GALLEON_ENTRY_MIN_SOMPI,
    GALLEON_EXPLORER,
    entry_payload,
    parse_l2_address,
    txid_has_galleon_prefix,
)
from galleon_faucet import address_of, galleon_key, print_balance  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402
from tn10_rest import MIN_PRIORITY_FEE_SOMPI, spendable_entries, virtual_daa  # noqa: E402
from tn10_wallets import EXPLORER, WALLET_NAMES, address_from_key, key_for  # noqa: E402

NETWORK_ID = "testnet-10"
DEFAULT_GRIND_LIMIT = 5_000_000


async def make_client() -> RpcClient:
    rpc = (os.environ.get("KASPA_RPC_URL") or "").strip()
    if rpc:
        client = RpcClient(url=rpc, network_id=NETWORK_ID)
        print(f"RPC {rpc}")
    else:
        client = RpcClient(resolver=Resolver(), network_id=NETWORK_ID)
        print("RPC Resolver (public TN10)")
    await client.connect(strategy="fallback")
    return client


def grind_entry_tx(
    entries: list,
    change_address: Address,
    amount_sompi: int,
    key,
    l2: bytes,
    limit: int,
) -> tuple[object, int]:
    for nonce in range(limit):
        payload = entry_payload(l2, amount_sompi, nonce)
        built = create_transactions(
            network_id=NETWORK_ID,
            entries=entries,
            change_address=change_address,
            outputs=[PaymentOutput(Address(GALLEON_ENTRY_ADDRESS), amount_sompi)],
            payload=payload,
            priority_fee=MIN_PRIORITY_FEE_SOMPI,
        )
        pending = built["transactions"][0]
        pending.sign([key])
        tx = pending.transaction
        if txid_has_galleon_prefix(tx.id):
            return pending, nonce
    raise RuntimeError(f"no txid with 97b4 prefix in {limit} nonce attempts")


async def run_entry(
    wallet: str,
    kas: float,
    *,
    broadcast: bool,
    grind_limit: int,
    l2_hex: str | None,
) -> str:
    amount_sompi = int(kaspa_to_sompi(kas))
    if amount_sompi < GALLEON_ENTRY_MIN_SOMPI:
        raise RuntimeError(
            f"need >= {GALLEON_ENTRY_MIN_SOMPI} sompi (~1 tKAS); got {amount_sompi}"
        )
    key = key_for(wallet)
    from_addr = address_from_key(key)
    l2 = parse_l2_address(l2_hex or address_of(galleon_key()))
    print(f"wallet   {wallet}  {from_addr}")
    print(f"entry    {GALLEON_ENTRY_ADDRESS}")
    print(f"l2 recv  0x{l2.hex()}")
    print(f"amount   {kas} tKAS ({amount_sompi} sompi)")
    client = await make_client()
    try:
        utxos = await client.get_utxos_by_addresses({"addresses": [from_addr]})
        mature = spendable_entries(utxos.get("entries") or [], virtual_daa())
        total = sum(int(e["utxoEntry"]["amount"]) for e in mature)
        need = amount_sompi + MIN_PRIORITY_FEE_SOMPI
        if total < need:
            raise RuntimeError(
                f"{wallet} has {total} spendable sompi, need {need} "
                f"(amount + priority fee). Fund {from_addr} from TN10 faucet."
            )
        print(f"spendable {total} sompi from {len(mature)} UTXO(s)")
        started = time.monotonic()
        pending, nonce = grind_entry_tx(
            mature, Address(from_addr), amount_sompi, key, l2, grind_limit
        )
        txid = pending.transaction.id
        print(f"grind    nonce={nonce}  tries in {time.monotonic() - started:.1f}s")
        print(f"txid     {txid}")
        print(f"explorer {EXPLORER}/txs/{txid}")
        if not broadcast:
            print("dry-run OK — pass --broadcast to submit")
            return txid
        submitted = await pending.submit(client)
        print(f"submitted {submitted}")
        print(f"Galleon primary {address_of(galleon_key())}")
        print_balance(address_of(galleon_key()))
        print(f"L2 explorer {GALLEON_EXPLORER}/address/{address_of(galleon_key())}")
        return submitted
    finally:
        await client.disconnect()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="TN10 tKAS → Galleon iKAS via L1 Entry")
    parser.add_argument("--wallet", choices=WALLET_NAMES, default="dave")
    parser.add_argument("--kas", type=float, default=1.0, help="tKAS to lock (min 1)")
    parser.add_argument("--l2", default="", help="0x L2 recipient (default GALLEON_PRIVATE_KEY)")
    parser.add_argument("--broadcast", action="store_true")
    parser.add_argument("--dry-run", action="store_true", help="grind only; no submit")
    parser.add_argument("--grind-limit", type=int, default=DEFAULT_GRIND_LIMIT)
    return parser.parse_args()


def main() -> int:
    load_kaspa_env(ROOT)
    args = parse_args()
    if args.broadcast and args.dry_run:
        raise SystemExit("use --broadcast or --dry-run, not both")
    asyncio.run(
        run_entry(
            args.wallet,
            args.kas,
            broadcast=args.broadcast and not args.dry_run,
            grind_limit=args.grind_limit,
            l2_hex=args.l2 or None,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
