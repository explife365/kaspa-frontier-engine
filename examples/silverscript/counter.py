"""TN10 SilverScript Counter covenant: genesis + add(5) + subtract(3).

Uses the official kaspa-python-sdk Counter contract. Each step is a real
testnet-10 transaction. Prints txids and writes .local/tn10-covenant-proof.json.

  python examples/silverscript/counter.py --print-address
  python examples/silverscript/counter.py

Secrets live in repo-root kaspa.env (gitignored). Optional process env:
    KASPA_RPC_URL       e.g. ws://127.0.0.1:17210  (else public Resolver)
    KASPA_FUNDING_KEY / KASPA_TN10_FUNDING_KEY   hex private key (recovery)

Fund the printed kaspatest: address from https://faucet-tn10.kaspanet.io/
Send at least 1 tKAS. Leave this process running.

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
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path

from kaspa import (
    Address,
    CovenantBinding,
    GenesisCovenantGroup,
    Hash,
    Keypair,
    PrivateKey,
    Resolver,
    RpcClient,
    ScriptBuilder,
    ScriptPublicKey,
    Transaction,
    TransactionInput,
    TransactionOutpoint,
    TransactionOutput,
    UtxoEntryReference,
    address_from_script_public_key,
    calculate_transaction_mass,
    sign_transaction,
)
import kaspa.experimental.silverscript as silverscript

NETWORK_ID = "testnet-10"
NETWORK_TYPE = "testnet"
SUBNETWORK_ID = bytes(20)
TX_VERSION = 1
COMPUTE_BUDGET = 10
GRAMS_PER_COMPUTE_BUDGET_UNIT = 100
FEE_MASS_SLACK = 200
MIN_FUNDING_SOMPI = 100_000_000
FUNDS_TIMEOUT_S = 45 * 60
ACCEPT_TIMEOUT_S = 180
SUBMIT_RETRIES = 3
EXPLORER = "https://explorer-tn10.kaspa.org"
FAUCET = "https://faucet-tn10.kaspanet.io/"
FLOW = ("genesis", "add(5)", "subtract(3)")

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from kaspa_env import load_kaspa_env, upsert_kaspa_env  # noqa: E402
from tn10_rest import is_mature_utxo, virtual_daa  # noqa: E402

LOCAL = ROOT / ".local"
PROOF_PATH = LOCAL / "tn10-covenant-proof.json"
load_kaspa_env(ROOT)
RPC_URL = (os.environ.get("KASPA_RPC_URL") or "").strip() or None

SOURCE = """
pragma silverscript ^0.1.0;

contract Counter(int init_count) {
 int count = init_count;

 #[covenant(binding = auth, from = 1, to = 1, mode = transition)]
 function add(State prev_state, int amount) : (State) {
 return({ count: prev_state.count + amount });
 }

 #[covenant(binding = auth, from = 1, to = 1, mode = transition)]
 function subtract(State prev_state, int amount) : (State) {
 require(prev_state.count - amount >= 0);
 return({ count: prev_state.count - amount });
 }
}
"""


def require_toccata_sdk() -> None:
    """Fail before funding/broadcast if the SDK drops v1 computeBudget."""
    probe = TransactionInput(
        TransactionOutpoint(Hash("00" * 32), 0),
        b"",
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
    )
    encoded = probe.to_dict()
    if encoded.get("computeBudget") != COMPUTE_BUDGET:
        raise RuntimeError(
            "installed kaspa-python-sdk drops computeBudget during serialization; "
            "refusing to fund or broadcast a broken Toccata v1 transaction. "
            "Use a published wheel that includes kaspa-python-sdk#78, then rerun."
        )


def lock_script(count: int) -> ScriptPublicKey:
    redeem = compiled_counter(count).script
    return ScriptBuilder.from_script(redeem, covenants_enabled=True).create_pay_to_script_hash_script()


@lru_cache(maxsize=16)
def compiled_counter(count: int):
    return silverscript.compile(SOURCE, [count])


def address(count: int) -> Address:
    return address_from_script_public_key(lock_script(count), NETWORK_TYPE)


def unlock_script(count: int, function: str, amount: int) -> bytes:
    contract = compiled_counter(count)
    call = contract.build_sig_script_for_covenant_decl(function, [amount])
    redeem = bytes.fromhex(
        ScriptBuilder(covenants_enabled=True).add_data(contract.script).to_string()
    )
    return call + redeem


@dataclass
class Counter:
    txid: str
    count: int
    value: int
    covenant_id: str
    live_utxo: dict | None = field(default=None, repr=False)

    @property
    def outpoint(self) -> TransactionOutpoint:
        return TransactionOutpoint(Hash(self.txid), 0)

    @property
    def utxo(self) -> UtxoEntryReference:
        spk = lock_script(self.count)
        return UtxoEntryReference.from_dict({
            "address": address(self.count).to_string(),
            "outpoint": {"transactionId": self.txid, "index": 0},
            "utxoEntry": {
                "amount": self.value,
                "scriptPublicKey": {"version": spk.version, "script": spk.script},
                "blockDaaScore": 0,
                "isCoinbase": False,
                "covenantId": self.covenant_id,
            },
        })


async def make_client() -> RpcClient:
    if RPC_URL:
        client = RpcClient(url=RPC_URL, network_id=NETWORK_ID)
        print(f"RPC {RPC_URL}")
    else:
        client = RpcClient(resolver=Resolver(), network_id=NETWORK_ID)
        print("RPC Resolver (public TN10)")
    await client.connect(strategy="fallback")
    return client


async def build_counter_tx(
    client: RpcClient,
    spend: TransactionInput,
    value_in: int,
    count: int,
    covenant: CovenantBinding | None,
) -> tuple[Transaction, int]:
    spk = lock_script(count)
    estimate = await client.get_fee_estimate()
    feerate = int(estimate["estimate"]["priorityBucket"]["feerate"])
    fee = 0
    mass = 0
    for _ in range(5):
        value_out = value_in - fee
        draft = Transaction(
            TX_VERSION, [spend], [TransactionOutput(value_out, spk, covenant)],
            lock_time=0, subnetwork_id=SUBNETWORK_ID, gas=0, payload=b"", mass=0,
        )
        if covenant is None:
            draft.populate_genesis_covenants([GenesisCovenantGroup(authorizing_input=0, outputs=[0])])
        mass = calculate_transaction_mass(NETWORK_ID, draft)
        fee_mass = mass + GRAMS_PER_COMPUTE_BUDGET_UNIT * COMPUTE_BUDGET + FEE_MASS_SLACK
        new_fee = fee_mass * feerate
        if new_fee == fee:
            break
        fee = new_fee
    if fee >= value_in:
        raise RuntimeError(f"fee {fee} sompi exceeds input {value_in}")
    value_out = value_in - fee
    tx = Transaction(
        TX_VERSION, [spend], [TransactionOutput(value_out, spk, covenant)],
        lock_time=0, subnetwork_id=SUBNETWORK_ID, gas=0, payload=b"", mass=mass,
    )
    return tx, value_out


async def genesis(client: RpcClient, funder_key: PrivateKey, funding_utxos: list[dict]) -> Counter:
    funding = max(funding_utxos, key=utxo_amount)
    spend = TransactionInput(
        TransactionOutpoint(Hash(funding["outpoint"]["transactionId"]), funding["outpoint"]["index"]),
        b"",
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
        utxo=UtxoEntryReference.from_dict(funding),
    )
    tx, value = await build_counter_tx(client, spend, utxo_amount(funding), count=0, covenant=None)
    tx.populate_genesis_covenants([GenesisCovenantGroup(authorizing_input=0, outputs=[0])])
    covenant_id = tx.outputs[0].to_dict()["covenant"]["covenantId"]
    signed = sign_transaction(tx, [funder_key], True)
    result = await submit_transaction(client, {"transaction": signed, "allowOrphan": False})
    return Counter(result["transactionId"], count=0, value=value, covenant_id=covenant_id)


async def transition(client: RpcClient, counter: Counter, function: str, amount: int) -> Counter:
    new_count = counter.count + amount if function == "add" else counter.count - amount
    spend_utxo = (
        UtxoEntryReference.from_dict(counter.live_utxo)
        if counter.live_utxo
        else counter.utxo
    )
    spend = TransactionInput(
        counter.outpoint,
        unlock_script(counter.count, function, amount),
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
        utxo=spend_utxo,
    )
    binding = CovenantBinding(authorizing_input=0, covenant_id=Hash(counter.covenant_id))
    tx, value = await build_counter_tx(client, spend, counter.value, new_count, binding)
    result = await submit_transaction(client, {"transaction": tx, "allowOrphan": False})
    return Counter(result["transactionId"], count=new_count, value=value, covenant_id=counter.covenant_id)


async def submit_transaction(client: RpcClient, payload: dict) -> dict:
    last: BaseException | None = None
    for attempt in range(SUBMIT_RETRIES):
        try:
            return await client.submit_transaction(payload)
        except Exception as err:
            last = err
            print(f"submit retry {attempt + 1}/{SUBMIT_RETRIES}: {err}")
            await asyncio.sleep(1.5 * (attempt + 1))
    assert last is not None
    raise last


def utxo_amount(entry: dict) -> int:
    return int(entry["utxoEntry"]["amount"])


async def wait_for_funds(client: RpcClient, addr: Address) -> list[dict]:
    deadline = time.monotonic() + FUNDS_TIMEOUT_S
    while True:
        result = await client.get_utxos_by_addresses({"addresses": [addr]})
        daa = virtual_daa()
        funded = [
            e
            for e in result["entries"]
            if utxo_amount(e) >= MIN_FUNDING_SOMPI and is_mature_utxo(e, daa)
        ]
        if funded:
            return funded
        if time.monotonic() >= deadline:
            raise TimeoutError(f"no faucet UTXO (>= 1 tKAS) after {FUNDS_TIMEOUT_S}s for {addr}")
        immature = [
            e
            for e in result["entries"]
            if utxo_amount(e) >= MIN_FUNDING_SOMPI and not is_mature_utxo(e, daa)
        ]
        if immature:
            print(f"waiting for coinbase maturity (1000 DAA) at {addr} ...")
        else:
            print(f"waiting for faucet (>= 1 tKAS) to {addr} ...")
        await asyncio.sleep(2)


async def wait_until_accepted(client: RpcClient, counter: Counter) -> dict:
    addr = address(counter.count)
    deadline = time.monotonic() + ACCEPT_TIMEOUT_S
    while True:
        result = await client.get_utxos_by_addresses({"addresses": [addr]})
        for entry in result["entries"]:
            if entry["outpoint"]["transactionId"] == counter.txid:
                return entry
        if time.monotonic() >= deadline:
            raise TimeoutError(f"txid {counter.txid} not in UTXO set after {ACCEPT_TIMEOUT_S}s")
        await asyncio.sleep(1)


def attach_live_utxo(counter: Counter, entry: dict) -> Counter:
    counter.live_utxo = entry
    counter.value = utxo_amount(entry)
    return counter


def show_step(label: str, counter: Counter) -> None:
    print(label)
    print(f"  count     {counter.count}")
    print(f"  address   {address(counter.count)}")
    print(f"  covenant  {counter.covenant_id}")
    print(f"  value     {counter.value:,} sompi")
    print(f"  txid      {counter.txid}")
    print(f"  explorer  {EXPLORER}/txs/{counter.txid}")
    print()


def proof_step(name: str, counter: Counter) -> dict:
    return {
        "step": name,
        "count": counter.count,
        "txid": counter.txid,
        "covenant_id": counter.covenant_id,
        "output_index": 0,
        "explorer": f"{EXPLORER}/txs/{counter.txid}",
    }


def write_proof(steps: list[dict], funding_address: str) -> None:
    LOCAL.mkdir(exist_ok=True)
    body = {
        "network": NETWORK_ID,
        "explorer": EXPLORER,
        "funding_address": funding_address,
        "steps": steps,
    }
    PROOF_PATH.write_text(json.dumps(body, indent=2), encoding="utf-8")
    print(f"wrote {PROOF_PATH}")


def load_proof_steps() -> list[dict]:
    if not PROOF_PATH.is_file():
        return []
    body = json.loads(PROOF_PATH.read_text(encoding="utf-8"))
    steps = body.get("steps") or []
    if not isinstance(steps, list):
        return []
    return steps


def remaining_flow(steps: list[dict]) -> tuple[str, ...]:
    names = [str(s.get("step", "")) for s in steps]
    for i, expected in enumerate(FLOW):
        if i >= len(names):
            return FLOW[i:]
        if names[i] != expected:
            raise ValueError(f"proof step {i} is {names[i]!r}, expected {expected!r}")
    return ()


def ensure_explorer_urls(steps: list[dict]) -> tuple[list[dict], bool]:
    out: list[dict] = []
    changed = False
    for step in steps:
        item = dict(step)
        txid = str(item.get("txid") or "")
        if txid and not item.get("explorer"):
            item["explorer"] = f"{EXPLORER}/txs/{txid}"
            changed = True
        out.append(item)
    return out, changed


def load_or_create_funder() -> tuple[PrivateKey, Address]:
    env_key = os.environ.get("KASPA_FUNDING_KEY") or os.environ.get("KASPA_TN10_FUNDING_KEY")
    if env_key:
        funder_key = PrivateKey(env_key)
        funding_address = funder_key.to_public_key().to_address(NETWORK_TYPE)
        return funder_key, funding_address
    keypair = Keypair.random()
    funder_key = PrivateKey(keypair.private_key)
    funding_address = keypair.to_address(NETWORK_TYPE)
    hex_key = str(keypair.private_key)
    upsert_kaspa_env(
        {
            "KASPA_TN10_FUNDING_KEY": hex_key,
            "KASPA_FUNDING_KEY": hex_key,
        },
        ROOT,
    )
    return funder_key, funding_address


async def recover_last(client: RpcClient, last: dict) -> Counter:
    counter = Counter(
        str(last["txid"]),
        int(last["count"]),
        0,
        str(last["covenant_id"]),
    )
    addr = address(counter.count)
    result = await client.get_utxos_by_addresses({"addresses": [addr]})
    for entry in result["entries"]:
        if entry["outpoint"]["transactionId"] == counter.txid:
            attach_live_utxo(counter, entry)
            return counter
    raise RuntimeError(
        f"proof txid {counter.txid} is not in the UTXO set "
        f"(spent, reorged, or not yet indexed). {EXPLORER}/txs/{counter.txid}"
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="TN10 SilverScript Counter covenant")
    parser.add_argument(
        "--print-address",
        action="store_true",
        help="print kaspatest: faucet address and exit (no RPC)",
    )
    parser.add_argument(
        "--no-resume",
        action="store_true",
        help="ignore .local/tn10-covenant-proof.json and start from genesis",
    )
    return parser.parse_args()


async def main() -> None:
    args = parse_args()
    LOCAL.mkdir(exist_ok=True)
    funder_key, funding_address = load_or_create_funder()
    funding_text = str(funding_address)
    if not funding_text.startswith("kaspatest:"):
        raise RuntimeError(f"TN10 counter refuses non-testnet funding address: {funding_text}")

    print("Fund this address with at least 1 tKAS (testnet only):")
    print(f"  {funding_address}")
    print(f"Faucet: {FAUCET}")
    print("Recovery key saved in kaspa.env (gitignored, testnet only)\n")
    if args.print_address:
        return

    require_toccata_sdk()
    steps = [] if args.no_resume else load_proof_steps()
    remaining = remaining_flow(steps)
    if not remaining:
        steps, changed = ensure_explorer_urls(steps)
        if changed:
            write_proof(steps, funding_text)
        print("Proof already complete:")
        for step in steps:
            print(f"  {step.get('step')}  {step.get('explorer') or step.get('txid')}")
        return

    print(f"SilverScript Counter on {NETWORK_ID}  remaining={list(remaining)}\n")
    client = await make_client()
    print("connected\n")
    try:
        counter: Counter | None = None
        if steps:
            print(f"resuming after {steps[-1].get('step')} {steps[-1].get('txid')}")
            counter = await recover_last(client, steps[-1])
            show_step(f"recovered count = {counter.count}", counter)

        if remaining[0] == "genesis":
            funding_utxos = await wait_for_funds(client, funding_address)
            counter = await genesis(client, funder_key, funding_utxos)
            attach_live_utxo(counter, await wait_until_accepted(client, counter))
            show_step("genesis count = 0", counter)
            steps.append(proof_step("genesis", counter))
            write_proof(steps, funding_text)
            remaining = remaining[1:]

        if counter is None:
            raise RuntimeError("no counter state to continue (corrupt proof?)")

        if "add(5)" in remaining:
            prev = counter.count
            counter = await transition(client, counter, "add", 5)
            attach_live_utxo(counter, await wait_until_accepted(client, counter))
            show_step(f"add(5) count {prev} -> {counter.count}", counter)
            steps.append(proof_step("add(5)", counter))
            write_proof(steps, funding_text)

        if "subtract(3)" in remaining:
            prev = counter.count
            counter = await transition(client, counter, "subtract", 3)
            attach_live_utxo(counter, await wait_until_accepted(client, counter))
            show_step(f"subtract(3) count {prev} -> {counter.count}", counter)
            steps.append(proof_step("subtract(3)", counter))
            write_proof(steps, funding_text)

        print(f"Final count = {counter.count}")
        print(f"Explorer: {EXPLORER}/txs/{counter.txid}")
    finally:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
