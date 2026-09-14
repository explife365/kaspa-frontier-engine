"""Lock TN10 tKAS via Galleon L1 Entry (txid prefix 97b4) to mint iKAS on Galleon.

  python examples/galleon_entry.py --dry-run --wallet bob --kas 1
  python examples/galleon_entry.py --wallet dave --kas 1 --broadcast

Requires >= 1 tKAS spendable on the named TN10 wallet. Grinds payload nonce until
the signed txid starts with 97b4 (Igra requirement). Posts on KIP-21 lane
(subnetwork 97b10000…, Toccata v1, computeBudget 10/input). Does not use the faucet.
"""

from __future__ import annotations

import argparse
import asyncio
import os
import sys
import time
from pathlib import Path

from kaspa import (
    Address,
    Hash,
    Resolver,
    RpcClient,
    Transaction,
    TransactionInput,
    TransactionOutput,
    TransactionOutpoint,
    UtxoEntryReference,
    calculate_transaction_mass,
    kaspa_to_sompi,
    pay_to_address_script,
    sign_transaction,
)

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "examples"))
from galleon import (  # noqa: E402
    GALLEON_ENTRY_ADDRESS,
    GALLEON_ENTRY_MIN_SOMPI,
    GALLEON_EXPLORER,
    IGRA_ENTRY_COMPUTE_BUDGET,
    IGRA_ENTRY_LANE_SUBNETWORK_ID,
    IGRA_ENTRY_TX_VERSION,
    entry_payload,
    parse_l2_address,
    txid_has_galleon_prefix,
)
from galleon_faucet import address_of, galleon_key, print_balance  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402
from kaspa_sdk_dev_patch import dev_patch_enabled, ensure_dev_patch_if_enabled  # noqa: E402
from tn10_rest import MIN_PRIORITY_FEE_SOMPI, spendable_entries, virtual_daa  # noqa: E402
from tn10_wallets import EXPLORER, WALLET_NAMES, address_from_key, key_for  # noqa: E402

NETWORK_ID = "testnet-10"
DEFAULT_GRIND_LIMIT = 5_000_000
GRAMS_PER_COMPUTE_BUDGET_UNIT = 100
FEE_MASS_SLACK = 200


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


def require_toccata_sdk() -> None:
    ensure_dev_patch_if_enabled()
    probe = TransactionInput(
        TransactionOutpoint(Hash("00" * 32), 0),
        b"",
        sequence=0,
        sig_op_count=0,
        compute_budget=IGRA_ENTRY_COMPUTE_BUDGET,
    )
    encoded = probe.to_dict()
    if encoded.get("computeBudget") != IGRA_ENTRY_COMPUTE_BUDGET:
        dev_hint = (
            " Set TN10_SDK_DEV_PATCH=1 for TN10 rehearsal until kaspa-python-sdk#78 publishes."
            if not dev_patch_enabled()
            else ""
        )
        raise RuntimeError(
            "kaspa-python-sdk drops computeBudget during serialization; "
            "refusing to broadcast a v0/native Entry (tKAS locks but iKAS never credits)."
            + dev_hint
        )


def funding_inputs(entries: list) -> list[TransactionInput]:
    return [
        TransactionInput(
            TransactionOutpoint(
                Hash(entry["outpoint"]["transactionId"]),
                entry["outpoint"]["index"],
            ),
            b"",
            sequence=0,
            sig_op_count=0,
            compute_budget=IGRA_ENTRY_COMPUTE_BUDGET,
            utxo=UtxoEntryReference.from_dict(entry),
        )
        for entry in entries
    ]


async def rpc_priority_feerate(client: RpcClient) -> int:
    estimate = await client.get_fee_estimate()
    return int(estimate["estimate"]["priorityBucket"]["feerate"])


async def build_lane_entry_tx(
    client: RpcClient,
    entries: list,
    change_address: Address,
    amount_sompi: int,
    payload: bytes,
    *,
    feerate: int | None = None,
) -> Transaction:
    """Toccata v1 Entry on Igra KIP-21 lane. Entry output must be index 0."""
    inputs = funding_inputs(entries)
    total_in = sum(int(e["utxoEntry"]["amount"]) for e in entries)
    entry_spk = pay_to_address_script(Address(GALLEON_ENTRY_ADDRESS))
    change_spk = pay_to_address_script(change_address)
    if feerate is None:
        feerate = await rpc_priority_feerate(client)
    fee = 0
    mass = 0
    budget_mass = (
        GRAMS_PER_COMPUTE_BUDGET_UNIT
        * IGRA_ENTRY_COMPUTE_BUDGET
        * len(inputs)
    )
    for _ in range(5):
        change = total_in - amount_sompi - fee
        if change < 0:
            raise RuntimeError(
                f"insufficient sompi after fee: in={total_in} amount={amount_sompi} fee={fee}"
            )
        outputs = [
            TransactionOutput(amount_sompi, entry_spk),
            TransactionOutput(change, change_spk),
        ]
        draft = Transaction(
            IGRA_ENTRY_TX_VERSION,
            inputs,
            outputs,
            lock_time=0,
            subnetwork_id=IGRA_ENTRY_LANE_SUBNETWORK_ID,
            gas=0,
            payload=payload,
            mass=0,
        )
        mass = calculate_transaction_mass(NETWORK_ID, draft)
        new_fee = (mass + budget_mass + FEE_MASS_SLACK) * feerate
        if new_fee == fee:
            break
        fee = new_fee
    if fee >= total_in - amount_sompi:
        raise RuntimeError(f"fee {fee} sompi leaves no change from {total_in} input")
    change = total_in - amount_sompi - fee
    return Transaction(
        IGRA_ENTRY_TX_VERSION,
        inputs,
        [
            TransactionOutput(amount_sompi, entry_spk),
            TransactionOutput(change, change_spk),
        ],
        lock_time=0,
        subnetwork_id=IGRA_ENTRY_LANE_SUBNETWORK_ID,
        gas=0,
        payload=payload,
        mass=mass,
    )


async def grind_entry_tx(
    client: RpcClient,
    entries: list,
    change_address: Address,
    amount_sompi: int,
    key,
    l2: bytes,
    limit: int,
) -> tuple[Transaction, int]:
    feerate = await rpc_priority_feerate(client)
    for nonce in range(limit):
        payload = entry_payload(l2, amount_sompi, nonce)
        tx = await build_lane_entry_tx(
            client, entries, change_address, amount_sompi, payload, feerate=feerate
        )
        signed = sign_transaction(tx, [key], True)
        if txid_has_galleon_prefix(signed.id):
            return signed, nonce
    raise RuntimeError(f"no txid with 97b4 prefix in {limit} nonce attempts")


async def submit_transaction(client: RpcClient, tx: Transaction) -> str:
    result = await client.submit_transaction({"transaction": tx, "allowOrphan": False})
    return result["transactionId"]


async def run_entry(
    wallet: str,
    kas: float,
    *,
    broadcast: bool,
    grind_limit: int,
    l2_hex: str | None,
) -> str:
    require_toccata_sdk()
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
    print(f"lane     KIP-21 {IGRA_ENTRY_LANE_SUBNETWORK_ID.hex()}")
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
        signed, nonce = await grind_entry_tx(
            client, mature, Address(from_addr), amount_sompi, key, l2, grind_limit
        )
        txid = signed.id
        print(f"grind    nonce={nonce}  tries in {time.monotonic() - started:.1f}s")
        print(f"txid     {txid}")
        meta = signed.to_dict()
        print(
            f"version  {meta.get('version')}  "
            f"subnetwork={meta.get('subnetworkId')}  "
            f"computeBudget={signed.inputs[0].to_dict().get('computeBudget')}"
        )
        print(f"explorer {EXPLORER}/txs/{txid}")
        if not broadcast:
            print("dry-run OK — pass --broadcast to submit")
            return txid
        submitted = await submit_transaction(client, signed)
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
    ensure_dev_patch_if_enabled()
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
