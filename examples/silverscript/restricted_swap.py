"""TN10 SilverScript restricted swap: genesis + single authorized recipient transition.

Only a pre-committed recipient hash may complete the swap transition. Pairs with the
off-chain destination policy in src/covenant.rs. Broadcast blocked until SDK #78.

  python examples/silverscript/restricted_swap.py --print-address
  python examples/silverscript/restricted_swap.py
"""

from __future__ import annotations

import argparse
import asyncio
import json
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path

from kaspa import (
    Address,
    CovenantBinding,
    GenesisCovenantGroup,
    Hash,
    PrivateKey,
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

from covenant_common import (
    COMPUTE_BUDGET,
    EXPLORER,
    FAUCET,
    FEE_MASS_SLACK,
    GRAMS_PER_COMPUTE_BUDGET_UNIT,
    LOCAL,
    NETWORK_ID,
    NETWORK_TYPE,
    ROOT,
    SUBNETWORK_ID,
    TX_VERSION,
    ensure_explorer_urls,
    load_or_create_funder,
    load_proof_steps,
    make_client,
    proof_step,
    remaining_flow,
    require_toccata_sdk,
    submit_transaction,
    utxo_amount,
    wait_for_funds,
    wait_until_accepted,
)

# Demo recipient tag (int covenant param). Production apps should commit a full hash.
ALLOWED_RECIPIENT_HASH = 0x5357_4150  # ASCII "SWAP"
FLOW = ("genesis", "swap")
PROOF_PATH = LOCAL / "tn10-swap-proof.json"
FIXTURE_PROOF = ROOT / "fixtures" / "tn10-swap-proof.json"

SOURCE = """
pragma silverscript ^0.1.0;

contract RestrictedSwap(int allowed_recipient_hash) {
 bool swapped = false;

 #[covenant(binding = auth, from = 1, to = 1, mode = transition)]
 function swap(State prev_state, int recipient_hash) : (State) {
 require(!prev_state.swapped);
 require(recipient_hash == allowed_recipient_hash);
 return({ swapped: true });
 }
}
"""


@lru_cache(maxsize=8)
def compiled_swap(allowed_recipient_hash: int) -> silverscript.CompiledContract:
    return silverscript.compile(SOURCE, [allowed_recipient_hash])


def lock_script(allowed_recipient_hash: int, swapped: bool) -> ScriptPublicKey:
    redeem = compiled_swap(allowed_recipient_hash).script
    return ScriptBuilder.from_script(redeem, covenants_enabled=True).create_pay_to_script_hash_script()


def swap_address(allowed_recipient_hash: int, swapped: bool = False) -> Address:
    return address_from_script_public_key(
        lock_script(allowed_recipient_hash, swapped), NETWORK_TYPE
    )


def unlock_script(allowed_recipient_hash: int, swapped: bool, recipient_hash: int) -> bytes:
    contract = compiled_swap(allowed_recipient_hash)
    call = contract.build_sig_script_for_covenant_decl("swap", [recipient_hash])
    redeem = bytes.fromhex(
        ScriptBuilder(covenants_enabled=True).add_data(contract.script).to_string()
    )
    return call + redeem


@dataclass
class SwapState:
    txid: str
    swapped: bool
    value: int
    covenant_id: str
    allowed_recipient_hash: int
    live_utxo: dict | None = field(default=None, repr=False)

    @property
    def state(self) -> int:
        return 1 if self.swapped else 0

    @property
    def outpoint(self) -> TransactionOutpoint:
        return TransactionOutpoint(Hash(self.txid), 0)

    @property
    def utxo(self) -> UtxoEntryReference:
        spk = lock_script(self.allowed_recipient_hash, self.swapped)
        return UtxoEntryReference.from_dict({
            "address": swap_address(self.allowed_recipient_hash, self.swapped).to_string(),
            "outpoint": {"transactionId": self.txid, "index": 0},
            "utxoEntry": {
                "amount": self.value,
                "scriptPublicKey": {"version": spk.version, "script": spk.script},
                "blockDaaScore": 0,
                "isCoinbase": False,
                "covenantId": self.covenant_id,
            },
        })


async def build_swap_tx(
    client: RpcClient,
    spend: TransactionInput,
    value_in: int,
    allowed_recipient_hash: int,
    swapped: bool,
    covenant: CovenantBinding | None,
) -> tuple[Transaction, int]:
    spk = lock_script(allowed_recipient_hash, swapped)
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


async def genesis(
    client: RpcClient,
    funder_key: PrivateKey,
    funding_utxos: list[dict],
    allowed_recipient_hash: int,
) -> SwapState:
    funding = max(funding_utxos, key=utxo_amount)
    spend = TransactionInput(
        TransactionOutpoint(Hash(funding["outpoint"]["transactionId"]), funding["outpoint"]["index"]),
        b"",
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
        utxo=UtxoEntryReference.from_dict(funding),
    )
    tx, value = await build_swap_tx(
        client, spend, utxo_amount(funding), allowed_recipient_hash, False, None
    )
    tx.populate_genesis_covenants([GenesisCovenantGroup(authorizing_input=0, outputs=[0])])
    covenant_id = tx.outputs[0].to_dict()["covenant"]["covenantId"]
    signed = sign_transaction(tx, [funder_key], True)
    result = await submit_transaction(client, {"transaction": signed, "allowOrphan": False})
    return SwapState(
        result["transactionId"],
        swapped=False,
        value=value,
        covenant_id=covenant_id,
        allowed_recipient_hash=allowed_recipient_hash,
    )


async def swap(client: RpcClient, state: SwapState) -> SwapState:
    recipient_hash = state.allowed_recipient_hash
    spend_utxo = state.live_utxo if state.live_utxo else state.utxo
    spend = TransactionInput(
        state.outpoint,
        unlock_script(state.allowed_recipient_hash, state.swapped, recipient_hash),
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
        utxo=spend_utxo if isinstance(spend_utxo, UtxoEntryReference) else UtxoEntryReference.from_dict(spend_utxo),
    )
    binding = CovenantBinding(authorizing_input=0, covenant_id=Hash(state.covenant_id))
    tx, value = await build_swap_tx(
        client, spend, state.value, state.allowed_recipient_hash, True, binding
    )
    result = await submit_transaction(client, {"transaction": tx, "allowOrphan": False})
    return SwapState(
        result["transactionId"],
        swapped=True,
        value=value,
        covenant_id=state.covenant_id,
        allowed_recipient_hash=state.allowed_recipient_hash,
    )


def attach_live_utxo(state: SwapState, entry: dict) -> SwapState:
    state.live_utxo = entry
    state.value = utxo_amount(entry)
    return state


def show_step(label: str, state: SwapState) -> None:
    print(label)
    print(f"  swapped    {state.swapped}")
    print(f"  recipient  hash {state.allowed_recipient_hash}")
    print(f"  address    {swap_address(state.allowed_recipient_hash, state.swapped)}")
    print(f"  covenant   {state.covenant_id}")
    print(f"  value      {state.value:,} sompi")
    print(f"  txid       {state.txid}")
    print(f"  explorer   {EXPLORER}/txs/{state.txid}")
    print()


async def recover_last(client: RpcClient, last: dict, allowed_recipient_hash: int) -> SwapState:
    state = SwapState(
        str(last["txid"]),
        bool(int(last.get("count", 0))),
        0,
        str(last["covenant_id"]),
        allowed_recipient_hash,
    )
    addr = swap_address(allowed_recipient_hash, state.swapped)
    result = await client.get_utxos_by_addresses({"addresses": [addr]})
    for entry in result["entries"]:
        if entry["outpoint"]["transactionId"] == state.txid:
            attach_live_utxo(state, entry)
            return state
    raise RuntimeError(f"proof txid {state.txid} not in UTXO set. {EXPLORER}/txs/{state.txid}")


def write_swap_proof(path: Path, steps: list[dict], funding_address: str) -> None:
    LOCAL.mkdir(exist_ok=True)
    body = {
        "app": "restricted_swap",
        "network": NETWORK_ID,
        "explorer": EXPLORER,
        "funding_address": funding_address,
        "allowed_recipient_hash": ALLOWED_RECIPIENT_HASH,
        "steps": steps,
    }
    path.write_text(json.dumps(body, indent=2), encoding="utf-8")
    print(f"wrote {path}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="TN10 restricted swap SilverScript reference")
    parser.add_argument("--print-address", action="store_true")
    parser.add_argument("--print-source", action="store_true")
    parser.add_argument("--no-resume", action="store_true")
    parser.add_argument("--publish-fixture", action="store_true")
    return parser.parse_args()


async def main() -> None:
    args = parse_args()
    if args.print_source:
        print(SOURCE.strip())
        return

    LOCAL.mkdir(exist_ok=True)
    funder_key, funding_address = load_or_create_funder()
    funding_text = str(funding_address)
    if not funding_text.startswith("kaspatest:"):
        raise RuntimeError(f"TN10 swap refuses non-testnet funding address: {funding_text}")

    print("Fund this address with at least 1 tKAS (testnet only):")
    print(f"  {funding_address}")
    print(f"Faucet: {FAUCET}\n")
    if args.print_address:
        return

    require_toccata_sdk()
    steps = [] if args.no_resume else load_proof_steps(PROOF_PATH)
    remaining = remaining_flow(steps, FLOW)
    if not remaining:
        steps, changed = ensure_explorer_urls(steps)
        if changed:
            write_swap_proof(PROOF_PATH, steps, funding_text)
        print("Proof already complete:")
        for step in steps:
            print(f"  {step.get('step')}  {step.get('explorer') or step.get('txid')}")
        if args.publish_fixture:
            FIXTURE_PROOF.write_text(PROOF_PATH.read_text(encoding="utf-8"), encoding="utf-8")
            print(f"published {FIXTURE_PROOF}")
        return

    print(
        f"RestrictedSwap on {NETWORK_ID}  allowed_hash={ALLOWED_RECIPIENT_HASH}  "
        f"remaining={list(remaining)}\n"
    )
    client = await make_client()
    print("connected\n")
    try:
        swap_state: SwapState | None = None
        if steps:
            swap_state = await recover_last(client, steps[-1], ALLOWED_RECIPIENT_HASH)
            show_step(f"recovered swapped={swap_state.swapped}", swap_state)

        if remaining[0] == "genesis":
            funding_utxos = await wait_for_funds(client, funding_address)
            swap_state = await genesis(client, funder_key, funding_utxos, ALLOWED_RECIPIENT_HASH)
            attach_live_utxo(
                swap_state,
                await wait_until_accepted(
                    client, swap_state.txid, swap_address(ALLOWED_RECIPIENT_HASH, False)
                ),
            )
            show_step("genesis (open)", swap_state)
            steps.append(
                proof_step("genesis", swap_state.txid, swap_state.covenant_id, swap_state.state)
            )
            write_swap_proof(PROOF_PATH, steps, funding_text)
            remaining = remaining[1:]

        if swap_state is None:
            raise RuntimeError("no swap state to continue (corrupt proof?)")

        if "swap" in remaining:
            swap_state = await swap(client, swap_state)
            attach_live_utxo(
                swap_state,
                await wait_until_accepted(
                    client, swap_state.txid, swap_address(ALLOWED_RECIPIENT_HASH, True)
                ),
            )
            show_step("swap (restricted recipient)", swap_state)
            steps.append(
                proof_step("swap", swap_state.txid, swap_state.covenant_id, swap_state.state)
            )
            write_swap_proof(PROOF_PATH, steps, funding_text)

        print(f"Final swapped = {swap_state.swapped}")
        if args.publish_fixture and len(steps) == len(FLOW):
            FIXTURE_PROOF.write_text(PROOF_PATH.read_text(encoding="utf-8"), encoding="utf-8")
            print(f"published {FIXTURE_PROOF}")
    finally:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
