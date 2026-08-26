"""Send tKAS between named TN10 wallets (alice / bob / carol / dave / eve).

  python examples/tn10_transfer.py --print-wallets
  python examples/tn10_transfer.py --from alice --to bob --kas 0.25
  python examples/tn10_transfer.py --from bob --to carol --kas 0.25 --conf 60
  python examples/tn10_transfer.py --topup

Keys stay in gitignored kaspa.env. This never prints private keys.

Independent dev sig / optional mainnet KAS (not the Kaspa Dev Fund):
    kaspa:qpxdemlyx445kt5xteux0qhadaw8lh5m0vnqvcy8fh483t70usgkkeulsx9cm
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import time
from pathlib import Path

from kaspa import (
    Address,
    PaymentOutput,
    Resolver,
    RpcClient,
    create_transactions,
    kaspa_to_sompi,
    sompi_to_kaspa,
)

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from kaspa_env import load_kaspa_env  # noqa: E402
from tn10_rest import (  # noqa: E402
    COINBASE_MATURITY_DAA,
    DEFAULT_CONFIRMATIONS,
    MIN_PRIORITY_FEE_SOMPI,
    MIN_STORAGE_SAFE_SOMPI,
    address_balance_sompi,
    address_utxos,
    assert_storage_mass_safe,
    confirm_withdrawal,
    fee_estimate,
    spendable_entries,
    utxo_amount,
    virtual_daa,
)
from tn10_wallets import (  # noqa: E402
    DEV_DONATION_ADDRESS,
    EXPLORER,
    WALLET_NAMES,
    address_from_key,
    ensure_wallets,
    get_wallet,
    key_for,
)

NETWORK_ID = "testnet-10"
ACCEPT_TIMEOUT_S = 90
WITHDRAW_TIMEOUT_S = 180
# Alice needs ~1 tKAS protocol fee to mint again; keep a bit of change.
MINT_HEADROOM_SOMPI = 120_000_000
SOURCE_RESERVE_SOMPI = 45_000_000
RECEIPT_PATH = ROOT / ".local" / "tn10-withdraw.json"
load_kaspa_env(ROOT)
RPC_URL = (os.environ.get("KASPA_RPC_URL") or "").strip() or None


async def make_client() -> RpcClient:
    if RPC_URL:
        client = RpcClient(url=RPC_URL, network_id=NETWORK_ID)
        print(f"RPC {RPC_URL}")
    else:
        client = RpcClient(resolver=Resolver(), network_id=NETWORK_ID)
        print("RPC Resolver (public TN10)")
    await client.connect(strategy="fallback")
    return client


def print_wallets() -> None:
    wallets = ensure_wallets(ROOT)
    print("TN10 test wallets (kaspatest: only). Keys are in kaspa.env.")
    print("Faucet: https://faucet-tn10.kaspanet.io/ (hCaptcha; no public claim API)")
    print(f"dev sig / optional mainnet KAS: {DEV_DONATION_ADDRESS}")
    print(f"  https://explorer.kaspa.org/addresses/{DEV_DONATION_ADDRESS}")
    for wallet in wallets:
        sompi = address_balance_sompi(wallet.address)
        if sompi is None:
            bal = "unknown (REST)"
        else:
            bal = f"{sompi_to_kaspa(sompi)} tKAS ({sompi} sompi)"
        print(f"  {wallet.name:5}  {wallet.address}")
        print(f"         balance {bal}")
        print(f"         {wallet.explorer}")
    fee = fee_estimate()
    if fee and isinstance(fee.get("priorityBucket"), dict):
        bucket = fee["priorityBucket"]
        print(
            f"fee estimate  {bucket.get('feerate')} sompi/gram  "
            f"(~{bucket.get('estimatedSeconds')}s)  standard relay policy target is 100"
        )


async def wait_for_output(client: RpcClient, address: str, txid: str) -> None:
    deadline = time.monotonic() + ACCEPT_TIMEOUT_S
    dest = Address(address)
    while True:
        result = await client.get_utxos_by_addresses({"addresses": [dest]})
        for entry in result["entries"]:
            if entry["outpoint"]["transactionId"] == txid:
                return
        if time.monotonic() >= deadline:
            raise TimeoutError(
                f"txid {txid} not in UTXO set for {address} after {ACCEPT_TIMEOUT_S}s"
            )
        await asyncio.sleep(1.5)


def wait_withdrawal_conf(
    dest: str, txid: str, output_index: int, amount_sompi: int, required: int
) -> dict:
    deadline = time.monotonic() + WITHDRAW_TIMEOUT_S
    need = max(1, required)
    while True:
        daa = virtual_daa()
        utxos = address_utxos(dest)
        if daa is not None:
            hit = confirm_withdrawal(
                txid, dest, output_index, amount_sompi, utxos, daa, need
            )
            if hit is not None:
                hit["dest"] = dest
                hit["virtual_daa"] = daa
                return hit
        if time.monotonic() >= deadline:
            raise TimeoutError(
                f"txid {txid} not confirmed to {need} DAA at {dest} after {WITHDRAW_TIMEOUT_S}s"
            )
        time.sleep(1.5)


def write_receipt(payload: dict) -> None:
    RECEIPT_PATH.parent.mkdir(parents=True, exist_ok=True)
    RECEIPT_PATH.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"receipt  {RECEIPT_PATH}")


async def send_kas(src_name: str, dest_name: str, kas: float, conf: int | None) -> str:
    if kas <= 0:
        raise ValueError("amount must be > 0")
    src = get_wallet(src_name, ROOT)
    dest = get_wallet(dest_name, ROOT)
    if src.address == dest.address:
        raise ValueError("refusing self-transfer")
    amount = int(kaspa_to_sompi(kas))
    if amount <= 0:
        raise ValueError("amount rounds to 0 sompi")

    key = key_for(src.name)
    from_addr = address_from_key(key)
    if from_addr != src.address:
        raise RuntimeError("wallet address mismatch")

    client = await make_client()
    print("connected")
    try:
        utxos = await client.get_utxos_by_addresses({"addresses": [from_addr]})
        entries = utxos.get("entries") or []
        mature = spendable_entries(entries, virtual_daa())
        skipped = len(entries) - len(mature)
        if skipped:
            print(f"skipped {skipped} immature coinbase UTXO(s) (need {COINBASE_MATURITY_DAA} DAA)")
        if not mature:
            raise RuntimeError(
                f"{src.name} has no spendable UTXOs. Fund {src.address} from the faucet "
                "and wait ~100s if the faucet output is coinbase."
            )
        fee = fee_estimate()
        if fee and isinstance(fee.get("priorityBucket"), dict):
            bucket = fee["priorityBucket"]
            print(
                f"fee estimate  {bucket.get('feerate')} sompi/gram  "
                f"(REST /info/fee-estimate; extra priority_fee {MIN_PRIORITY_FEE_SOMPI} sompi)"
            )
        inputs = [utxo_amount(entry) for entry in mature]
        total = sum(inputs)
        if amount + MIN_PRIORITY_FEE_SOMPI > total:
            raise RuntimeError(
                f"{src.name} has {total} spendable sompi, need {amount} plus "
                f"{MIN_PRIORITY_FEE_SOMPI} fee pad"
            )
        change = total - amount - MIN_PRIORITY_FEE_SOMPI
        outputs = [amount]
        if change > 0:
            outputs.append(change)
        assert_storage_mass_safe(inputs, outputs)
        built = create_transactions(
            network_id=NETWORK_ID,
            entries=mature,
            change_address=from_addr,
            outputs=[PaymentOutput(Address(dest.address), amount)],
            priority_fee=MIN_PRIORITY_FEE_SOMPI,
        )
        pending_list = built["transactions"]
        if not pending_list:
            raise RuntimeError("create_transactions returned no transactions")
        last_id = ""
        for pending in pending_list:
            pending.sign([key])
            last_id = await pending.submit(client)
            print(f"submitted {src.name} -> {dest.name}  {last_id}")
            print(f"  explorer  {EXPLORER}/txs/{last_id}")
        await wait_for_output(client, dest.address, last_id)
        print(f"accepted at {dest.name}  {dest.explorer}")
        if conf is not None:
            hit = wait_withdrawal_conf(dest.address, last_id, 0, amount, conf)
            print(
                f"CONFIRMED {hit['confirmations']} DAA  vout {hit['output_index']}  "
                f"{hit['amount_sompi']} sompi"
            )
            write_receipt(
                {
                    "from": src.name,
                    "to": dest.name,
                    "dest": dest.address,
                    "kas": kas,
                    **hit,
                    "explorer": f"{EXPLORER}/txs/{last_id}",
                }
            )
        return last_id
    finally:
        await client.disconnect()


def topup_plan() -> list[tuple[str, str, float]]:
    """Move mint headroom to alice from the richest other wallet. Storage-mass safe."""
    wallets = {w.name: w for w in ensure_wallets(ROOT)}
    bals: dict[str, int] = {}
    for name, wallet in wallets.items():
        sompi = address_balance_sompi(wallet.address)
        bals[name] = 0 if sompi is None else sompi
        print(f"  {name:5}  {sompi_to_kaspa(bals[name])} tKAS")
    alice = bals.get("alice", 0)
    need = MINT_HEADROOM_SOMPI - alice
    if need <= 0:
        print(f"alice already has mint headroom ({sompi_to_kaspa(alice)} tKAS)")
        return []
    donors = sorted(
        ((name, sompi) for name, sompi in bals.items() if name != "alice"),
        key=lambda item: item[1],
        reverse=True,
    )
    if not donors:
        return []
    src_name, src_bal = donors[0]
    sendable = src_bal - SOURCE_RESERVE_SOMPI - MIN_PRIORITY_FEE_SOMPI
    # Both dest and donor change must stay >= 0.2 tKAS.
    max_send = src_bal - MIN_STORAGE_SAFE_SOMPI - MIN_PRIORITY_FEE_SOMPI
    amount = min(need, sendable, max_send)
    if amount < MIN_STORAGE_SAFE_SOMPI:
        print(
            f"cannot top up alice from {src_name}: sendable {amount} sompi "
            f"(need >= {MIN_STORAGE_SAFE_SOMPI} for storage mass)"
        )
        return []
    kas = float(sompi_to_kaspa(amount))
    print(
        f"plan  {src_name} -> alice  {kas} tKAS  "
        f"(alice {sompi_to_kaspa(alice)} -> ~{sompi_to_kaspa(alice + amount)})"
    )
    return [(src_name, "alice", kas)]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="TN10 named-wallet transfer")
    parser.add_argument("--print-wallets", action="store_true")
    parser.add_argument("--topup", action="store_true", help="fund alice toward ~1.2 tKAS mint headroom")
    parser.add_argument("--from", dest="src", choices=WALLET_NAMES)
    parser.add_argument("--to", dest="dest", choices=WALLET_NAMES)
    parser.add_argument("--kas", type=float, help="amount in tKAS")
    parser.add_argument(
        "--conf",
        type=int,
        metavar="N",
        help=f"wait until dest UTXO has N DAA confirmations (default {DEFAULT_CONFIRMATIONS} if flag used)",
        nargs="?",
        const=DEFAULT_CONFIRMATIONS,
    )
    return parser.parse_args()


async def main() -> None:
    args = parse_args()
    if args.print_wallets:
        print_wallets()
        return
    if args.topup:
        print("Faucet is hCaptcha-gated; topping up from richest named wallet if storage-mass safe.")
        plan = topup_plan()
        if not plan:
            return
        for src, dest, kas in plan:
            await send_kas(src, dest, kas, conf=DEFAULT_CONFIRMATIONS)
        print_wallets()
        return
    if not args.src or not args.dest or args.kas is None:
        raise SystemExit(
            "usage: tn10_transfer.py --print-wallets | --topup | "
            "--from alice --to bob --kas 0.25 [--conf [N]]"
        )
    await send_kas(args.src, args.dest, args.kas, args.conf)


if __name__ == "__main__":
    asyncio.run(main())
