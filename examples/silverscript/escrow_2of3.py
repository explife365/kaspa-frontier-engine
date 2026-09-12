"""TN10 SilverScript 2-of-3 escrow: genesis + seller/arbiter release (buyer timeout path).

Demo uses int actor tags (not pubkeys). Off-chain policy mirror: src/covenant.rs Escrow2of3PolicyEngine.

  python examples/silverscript/escrow_2of3.py --print-address
  python examples/silverscript/escrow_2of3.py --short-timeout
"""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
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
    pay_to_address_script,
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
    FUNDS_TIMEOUT_S,
    GRAMS_PER_COMPUTE_BUDGET_UNIT,
    LOCAL,
    DEFAULT_GENESIS_LOCK_SOMPI,
    MIN_GENESIS_INPUT_SOMPI,
    MIN_GENESIS_UTXO_SOMPI,
    genesis_lock_split,
    NETWORK_ID,
    NETWORK_TYPE,
    ROOT,
    SUBNETWORK_ID,
    TX_VERSION,
    ensure_explorer_urls,
    funding_inputs,
    load_or_create_funder,
    load_proof_refund_daa,
    load_proof_steps,
    make_client,
    proof_step,
    remaining_flow,
    require_toccata_sdk,
    rpc_priority_feerate,
    rpc_virtual_daa,
    script_with_int_state,
    script_with_int_state_redeem,
    select_genesis_utxos,
    submit_transaction,
    utxo_amount,
    virtual_daa,
    wait_for_funds,
    wait_until_accepted,
)

sys.path.insert(0, str(ROOT / "scripts"))
from tn10_rest import is_mature_utxo  # noqa: E402
from tn10_wallets import WALLET_NAMES  # noqa: E402

BUYER_HASH = 0x4255_5945  # demo "BUYE"
SELLER_HASH = 0x5345_4C4C  # demo "SELL"
ARBITER_HASH = 0x4152_4220  # demo "ARB "
DAA_TIMEOUT_OFFSET = 86_400
SHORT_TIMEOUT_OFFSET = 18_000  # ~minutes on TN10; genesis+release must finish before refund_daa
FLOW = ("genesis", "release_seller")
PROOF_PATH = LOCAL / "tn10-escrow-2of3-proof.json"
FIXTURE_PROOF = ROOT / "fixtures" / "tn10-escrow-2of3-proof.json"

SOURCE = """
pragma silverscript ^0.1.0;

contract Escrow2of3(int buyer_hash, int seller_hash, int arbiter_hash, int refund_daa) {
 int status = 0;

 #[covenant(binding = auth, from = 1, to = 1, mode = transition)]
 function release_to_seller(State prev_state, int actor1, int actor2, int current_daa) : (State) {
 require(prev_state.status == 0);
 require(current_daa < refund_daa);
 require(
  (actor1 == seller_hash && actor2 == arbiter_hash) ||
  (actor1 == arbiter_hash && actor2 == seller_hash)
 );
 return({ status: 1 });
 }

 #[covenant(binding = auth, from = 1, to = 1, mode = transition)]
 function refund_to_buyer(State prev_state, int actor1, int actor2, int current_daa) : (State) {
 require(prev_state.status == 0);
 require(current_daa < refund_daa);
 require(
  (actor1 == buyer_hash && actor2 == arbiter_hash) ||
  (actor1 == arbiter_hash && actor2 == buyer_hash)
 );
 return({ status: 2 });
 }

 #[covenant(binding = auth, from = 1, to = 1, mode = transition)]
 function timeout_to_buyer(State prev_state, int current_daa) : (State) {
 require(prev_state.status == 0);
 require(current_daa >= refund_daa);
 return({ status: 3 });
 }
}
"""


@lru_cache(maxsize=8)
def compiled_escrow(
    buyer_hash: int, seller_hash: int, arbiter_hash: int, refund_daa: int
) -> silverscript.CompiledContract:
    return silverscript.compile(SOURCE, [buyer_hash, seller_hash, arbiter_hash, refund_daa])


def lock_script(
    buyer_hash: int,
    seller_hash: int,
    arbiter_hash: int,
    refund_daa: int,
    status: int,
) -> ScriptPublicKey:
    contract = compiled_escrow(buyer_hash, seller_hash, arbiter_hash, refund_daa)
    redeem = script_with_int_state(contract, status)
    return ScriptBuilder.from_script(redeem, covenants_enabled=True).create_pay_to_script_hash_script()


def escrow_address(
    buyer_hash: int,
    seller_hash: int,
    arbiter_hash: int,
    refund_daa: int,
    status: int = 0,
) -> Address:
    return address_from_script_public_key(
        lock_script(buyer_hash, seller_hash, arbiter_hash, refund_daa, status),
        NETWORK_TYPE,
    )


def unlock_release_seller(
    buyer_hash: int,
    seller_hash: int,
    arbiter_hash: int,
    refund_daa: int,
    status: int,
    actor1: int,
    actor2: int,
    current_daa: int,
    *,
    legacy_redeem: bool = False,
) -> bytes:
    contract = compiled_escrow(buyer_hash, seller_hash, arbiter_hash, refund_daa)
    call = contract.build_sig_script_for_covenant_decl(
        "release_to_seller", [actor1, actor2, current_daa]
    )
    state_bytes = (
        script_with_int_state_redeem(contract, status)
        if legacy_redeem and status == 0
        else script_with_int_state(contract, status)
    )
    redeem = bytes.fromhex(
        ScriptBuilder(covenants_enabled=True).add_data(state_bytes).to_string()
    )
    return call + redeem


@dataclass
class EscrowState:
    txid: str
    status: int
    value: int
    covenant_id: str
    buyer_hash: int
    seller_hash: int
    arbiter_hash: int
    refund_daa: int
    live_utxo: dict | None = field(default=None, repr=False)

    @property
    def outpoint(self) -> TransactionOutpoint:
        return TransactionOutpoint(Hash(self.txid), 0)

    @property
    def utxo(self) -> UtxoEntryReference:
        spk = lock_script(
            self.buyer_hash,
            self.seller_hash,
            self.arbiter_hash,
            self.refund_daa,
            self.status,
        )
        return UtxoEntryReference.from_dict({
            "address": escrow_address(
                self.buyer_hash,
                self.seller_hash,
                self.arbiter_hash,
                self.refund_daa,
                self.status,
            ).to_string(),
            "outpoint": {"transactionId": self.txid, "index": 0},
            "utxoEntry": {
                "amount": self.value,
                "scriptPublicKey": {"version": spk.version, "script": spk.script},
                "blockDaaScore": 0,
                "isCoinbase": False,
                "covenantId": self.covenant_id,
            },
        })


def legacy_lock_script(
    buyer_hash: int,
    seller_hash: int,
    arbiter_hash: int,
    refund_daa: int,
    status: int,
) -> ScriptPublicKey:
    contract = compiled_escrow(buyer_hash, seller_hash, arbiter_hash, refund_daa)
    redeem = script_with_int_state_redeem(contract, status)
    return ScriptBuilder.from_script(redeem, covenants_enabled=True).create_pay_to_script_hash_script()


def legacy_escrow_address(
    buyer_hash: int,
    seller_hash: int,
    arbiter_hash: int,
    refund_daa: int,
    status: int = 0,
) -> Address:
    return address_from_script_public_key(
        legacy_lock_script(buyer_hash, seller_hash, arbiter_hash, refund_daa, status),
        NETWORK_TYPE,
    )


def legacy_status0_spend(state: EscrowState) -> bool:
    """Pre-fix genesis outputs on legacy_escrow_address use untagged int redeem bytes."""
    if state.status != 0 or not state.live_utxo:
        return False
    addr = state.live_utxo.get("address") or ""
    return addr == str(
        legacy_escrow_address(
            state.buyer_hash,
            state.seller_hash,
            state.arbiter_hash,
            state.refund_daa,
            0,
        )
    )


async def build_escrow_tx(
    client: RpcClient,
    spends: list[TransactionInput],
    value_in: int,
    buyer_hash: int,
    seller_hash: int,
    arbiter_hash: int,
    refund_daa: int,
    status: int,
    covenant: CovenantBinding | None,
    *,
    max_lock_sompi: int | None = None,
    change_spk: ScriptPublicKey | None = None,
    min_lock_sompi: int = MIN_GENESIS_INPUT_SOMPI,
) -> tuple[Transaction, int]:
    spk = lock_script(buyer_hash, seller_hash, arbiter_hash, refund_daa, status)
    feerate = await rpc_priority_feerate(client)
    fee = 0
    mass = 0
    value_out = 0

    def draft_outputs(current_fee: int) -> list[TransactionOutput]:
        nonlocal value_out
        if covenant is None and max_lock_sompi is not None and change_spk is not None:
            lock, change = genesis_lock_split(
                value_in, current_fee, min_lock_sompi, max_lock_sompi
            )
            value_out = lock
            outputs = [TransactionOutput(lock, spk, covenant)]
            if change > 0:
                outputs.append(TransactionOutput(change, change_spk, None))
            return outputs
        value_out = value_in - current_fee
        return [TransactionOutput(value_out, spk, covenant)]

    for _ in range(5):
        if fee >= value_in:
            raise RuntimeError(f"fee {fee} sompi exceeds input {value_in}")
        draft = Transaction(
            TX_VERSION, spends, draft_outputs(fee),
            lock_time=0, subnetwork_id=SUBNETWORK_ID, gas=0, payload=b"", mass=0,
        )
        if covenant is None:
            draft.populate_genesis_covenants([GenesisCovenantGroup(authorizing_input=0, outputs=[0])])
        mass = calculate_transaction_mass(NETWORK_ID, draft)
        compute_units = sum(int(inp.compute_budget) for inp in spends)
        fee_mass = mass + GRAMS_PER_COMPUTE_BUDGET_UNIT * compute_units + FEE_MASS_SLACK
        new_fee = fee_mass * feerate
        if new_fee == fee:
            break
        fee = new_fee
    if fee >= value_in:
        raise RuntimeError(f"fee {fee} sompi exceeds input {value_in}")
    tx = Transaction(
        TX_VERSION, spends, draft_outputs(fee),
        lock_time=0, subnetwork_id=SUBNETWORK_ID, gas=0, payload=b"", mass=mass,
    )
    return tx, value_out


async def genesis(
    client: RpcClient,
    funder_key: PrivateKey,
    funding_utxos: list[dict],
    buyer_hash: int,
    seller_hash: int,
    arbiter_hash: int,
    refund_daa: int,
    min_input_sompi: int = MIN_GENESIS_INPUT_SOMPI,
    max_lock_sompi: int = DEFAULT_GENESIS_LOCK_SOMPI,
) -> EscrowState:
    selected = select_genesis_utxos(funding_utxos, min_input_sompi=min_input_sompi)
    spends = funding_inputs(selected)
    value_in = sum(utxo_amount(entry) for entry in selected)
    change_spk = pay_to_address_script(funder_key.to_public_key().to_address(NETWORK_TYPE))
    tx, value = await build_escrow_tx(
        client,
        spends,
        value_in,
        buyer_hash,
        seller_hash,
        arbiter_hash,
        refund_daa,
        0,
        None,
        max_lock_sompi=max_lock_sompi,
        change_spk=change_spk,
        min_lock_sompi=min_input_sompi,
    )
    tx.populate_genesis_covenants([GenesisCovenantGroup(authorizing_input=0, outputs=[0])])
    covenant_id = tx.outputs[0].to_dict()["covenant"]["covenantId"]
    signed = sign_transaction(tx, [funder_key], True)
    result = await submit_transaction(client, {"transaction": signed, "allowOrphan": False})
    return EscrowState(
        result["transactionId"],
        0,
        value,
        covenant_id,
        buyer_hash,
        seller_hash,
        arbiter_hash,
        refund_daa,
    )


def unlock_timeout_to_buyer(
    buyer_hash: int,
    seller_hash: int,
    arbiter_hash: int,
    refund_daa: int,
    status: int,
    current_daa: int,
    *,
    legacy_redeem: bool = False,
) -> bytes:
    contract = compiled_escrow(buyer_hash, seller_hash, arbiter_hash, refund_daa)
    call = contract.build_sig_script_for_covenant_decl("timeout_to_buyer", [current_daa])
    state_bytes = (
        script_with_int_state_redeem(contract, status)
        if legacy_redeem and status == 0
        else script_with_int_state(contract, status)
    )
    redeem = bytes.fromhex(
        ScriptBuilder(covenants_enabled=True).add_data(state_bytes).to_string()
    )
    return call + redeem


async def timeout_to_buyer(client: RpcClient, state: EscrowState) -> EscrowState:
    current_daa = await rpc_virtual_daa(client)
    if current_daa < state.refund_daa:
        raise RuntimeError(
            f"timeout before refund_daa: need {state.refund_daa}, current {current_daa}"
        )
    spend_utxo = state.live_utxo if state.live_utxo else state.utxo
    spend = TransactionInput(
        state.outpoint,
        unlock_timeout_to_buyer(
            state.buyer_hash,
            state.seller_hash,
            state.arbiter_hash,
            state.refund_daa,
            state.status,
            current_daa,
            legacy_redeem=legacy_status0_spend(state),
        ),
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
        utxo=spend_utxo if isinstance(spend_utxo, UtxoEntryReference) else UtxoEntryReference.from_dict(spend_utxo),
    )
    binding = CovenantBinding(authorizing_input=0, covenant_id=Hash(state.covenant_id))
    tx, value = await build_escrow_tx(
        client,
        [spend],
        state.value,
        state.buyer_hash,
        state.seller_hash,
        state.arbiter_hash,
        state.refund_daa,
        3,
        binding,
    )
    result = await submit_transaction(client, {"transaction": tx, "allowOrphan": False})
    return EscrowState(
        result["transactionId"],
        3,
        value,
        state.covenant_id,
        state.buyer_hash,
        state.seller_hash,
        state.arbiter_hash,
        state.refund_daa,
    )


async def release_seller(client: RpcClient, state: EscrowState) -> EscrowState:
    current_daa = await rpc_virtual_daa(client)
    spend_utxo = state.live_utxo if state.live_utxo else state.utxo
    spend = TransactionInput(
        state.outpoint,
        unlock_release_seller(
            state.buyer_hash,
            state.seller_hash,
            state.arbiter_hash,
            state.refund_daa,
            state.status,
            state.seller_hash,
            state.arbiter_hash,
            current_daa,
            legacy_redeem=legacy_status0_spend(state),
        ),
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
        utxo=spend_utxo if isinstance(spend_utxo, UtxoEntryReference) else UtxoEntryReference.from_dict(spend_utxo),
    )
    binding = CovenantBinding(authorizing_input=0, covenant_id=Hash(state.covenant_id))
    tx, value = await build_escrow_tx(
        client,
        [spend],
        state.value,
        state.buyer_hash,
        state.seller_hash,
        state.arbiter_hash,
        state.refund_daa,
        1,
        binding,
    )
    result = await submit_transaction(client, {"transaction": tx, "allowOrphan": False})
    return EscrowState(
        result["transactionId"],
        1,
        value,
        state.covenant_id,
        state.buyer_hash,
        state.seller_hash,
        state.arbiter_hash,
        state.refund_daa,
    )


def attach_live_utxo(state: EscrowState, entry: dict) -> EscrowState:
    state.live_utxo = entry
    state.value = utxo_amount(entry)
    return state


def show_step(label: str, state: EscrowState) -> None:
    print(label)
    print(f"  status      {state.status}")
    print(f"  buyer       hash {state.buyer_hash}")
    print(f"  seller      hash {state.seller_hash}")
    print(f"  arbiter     hash {state.arbiter_hash}")
    print(f"  refund_daa  {state.refund_daa}")
    print(f"  address     {escrow_address(state.buyer_hash, state.seller_hash, state.arbiter_hash, state.refund_daa, state.status)}")
    print(f"  covenant    {state.covenant_id}")
    print(f"  value       {state.value:,} sompi")
    print(f"  txid        {state.txid}")
    print(f"  explorer    {EXPLORER}/txs/{state.txid}")
    print()


async def recover_last(client: RpcClient, last: dict, refund_daa: int) -> EscrowState:
    state = EscrowState(
        str(last["txid"]),
        int(last.get("count", 0)),
        0,
        str(last["covenant_id"]),
        BUYER_HASH,
        SELLER_HASH,
        ARBITER_HASH,
        refund_daa,
    )
    open_addr = (last.get("open_address") or "").strip()
    candidates = [open_addr] if open_addr else []
    candidates.append(
        str(escrow_address(BUYER_HASH, SELLER_HASH, ARBITER_HASH, refund_daa, state.status))
    )
    for addr in candidates:
        if not addr:
            continue
        result = await client.get_utxos_by_addresses({"addresses": [addr]})
        for entry in result["entries"]:
            if entry["outpoint"]["transactionId"] == state.txid:
                attach_live_utxo(state, entry)
                return state
    raise RuntimeError(f"proof txid {state.txid} not in UTXO set. {EXPLORER}/txs/{state.txid}")


async def preflight_genesis_funds(
    client: RpcClient, funding_address: Address, min_input_sompi: int
) -> None:
    """Fail fast when the funder has no mature genesis-sized inputs (avoids 45m wait)."""
    result = await client.get_utxos_by_addresses({"addresses": [str(funding_address)]})
    daa = virtual_daa()
    mature = [
        entry
        for entry in result["entries"]
        if is_mature_utxo(entry, daa) and utxo_amount(entry) >= MIN_GENESIS_UTXO_SOMPI
    ]
    total = sum(utxo_amount(entry) for entry in mature)
    if total >= min_input_sompi:
        return
    raise RuntimeError(
        f"funder has {total:,} mature sompi, need {min_input_sompi:,} for covenant genesis "
        f"(~0.5 tKAS). Fund {funding_address} at {FAUCET}"
    )


def write_escrow_proof(path: Path, steps: list[dict], funding_address: str, refund_daa: int) -> None:
    LOCAL.mkdir(exist_ok=True)
    body = {
        "app": "escrow_2of3",
        "network": NETWORK_ID,
        "explorer": EXPLORER,
        "funding_address": funding_address,
        "buyer_hash": BUYER_HASH,
        "seller_hash": SELLER_HASH,
        "arbiter_hash": ARBITER_HASH,
        "refund_daa": refund_daa,
        "steps": steps,
    }
    path.write_text(json.dumps(body, indent=2), encoding="utf-8")
    print(f"wrote {path}")


def load_funder(wallet: str | None) -> tuple[PrivateKey, Address]:
    if wallet:
        sys.path.insert(0, str(ROOT / "scripts"))
        from tn10_wallets import get_wallet, key_for

        key = key_for(wallet)
        addr = Address(get_wallet(wallet).address)
        return key, addr
    return load_or_create_funder()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="TN10 2-of-3 escrow SilverScript reference")
    parser.add_argument("--print-address", action="store_true")
    parser.add_argument("--print-source", action="store_true")
    parser.add_argument("--no-resume", action="store_true")
    parser.add_argument("--publish-fixture", action="store_true")
    parser.add_argument("--short-timeout", action="store_true", help="~12s dispute window for rehearsal")
    parser.add_argument(
        "--wallet",
        choices=WALLET_NAMES,
        help="named TN10 wallet for genesis funding (default: KASPA_TN10_FUNDING_KEY / alice)",
    )
    parser.add_argument(
        "--min-funding-sompi",
        type=int,
        default=MIN_GENESIS_INPUT_SOMPI,
        help=f"minimum mature input total (default {MIN_GENESIS_INPUT_SOMPI})",
    )
    parser.add_argument(
        "--max-lock-sompi",
        type=int,
        default=DEFAULT_GENESIS_LOCK_SOMPI,
        help=(
            f"max sompi locked in covenant at genesis; excess returns as change "
            f"(default {DEFAULT_GENESIS_LOCK_SOMPI} = 1 tKAS)"
        ),
    )
    return parser.parse_args()


async def main() -> None:
    args = parse_args()
    if args.print_source:
        print(SOURCE.strip())
        return

    LOCAL.mkdir(exist_ok=True)
    funder_key, funding_address = load_funder(args.wallet)
    funding_text = str(funding_address)
    if not funding_text.startswith("kaspatest:"):
        raise RuntimeError(f"TN10 escrow refuses non-testnet funding address: {funding_text}")

    offset = SHORT_TIMEOUT_OFFSET if args.short_timeout else DAA_TIMEOUT_OFFSET
    if args.no_resume:
        refund_daa = virtual_daa() + offset
    else:
        refund_daa = load_proof_refund_daa(PROOF_PATH)
        if refund_daa is None:
            refund_daa = virtual_daa() + offset

    print("Fund this address with at least 1 tKAS (testnet only):")
    print(f"  {funding_address}")
    print(f"Faucet: {FAUCET}")
    print(
        f"Escrow open (status=0): "
        f"{escrow_address(BUYER_HASH, SELLER_HASH, ARBITER_HASH, refund_daa, 0)}\n"
    )
    if args.print_address:
        return

    require_toccata_sdk()
    steps = [] if args.no_resume else load_proof_steps(PROOF_PATH)
    remaining = remaining_flow(steps, FLOW)
    if not remaining:
        steps, changed = ensure_explorer_urls(steps)
        if changed:
            write_escrow_proof(PROOF_PATH, steps, funding_text, refund_daa)
        print("Proof already complete:")
        for step in steps:
            print(f"  {step.get('step')}  {step.get('explorer') or step.get('txid')}")
        if args.publish_fixture:
            FIXTURE_PROOF.write_text(PROOF_PATH.read_text(encoding="utf-8"), encoding="utf-8")
            print(f"published {FIXTURE_PROOF}")
        return

    print(
        f"Escrow2of3 on {NETWORK_ID}  refund_daa={refund_daa}  remaining={list(remaining)}\n"
    )
    client = await make_client()
    print("connected\n")
    try:
        escrow_state: EscrowState | None = None
        if steps:
            escrow_state = await recover_last(client, steps[-1], refund_daa)
            show_step(f"recovered status={escrow_state.status}", escrow_state)

        if remaining[0] == "genesis":
            await preflight_genesis_funds(client, funding_address, args.min_funding_sompi)
            funds_timeout = 120 if args.short_timeout else FUNDS_TIMEOUT_S
            funding_utxos = await wait_for_funds(
                client, funding_address, min_funding_sompi=args.min_funding_sompi, timeout_s=funds_timeout
            )
            escrow_state = await genesis(
                client,
                funder_key,
                funding_utxos,
                BUYER_HASH,
                SELLER_HASH,
                ARBITER_HASH,
                refund_daa,
                min_input_sompi=args.min_funding_sompi,
                max_lock_sompi=args.max_lock_sompi,
            )
            attach_live_utxo(
                escrow_state,
                await wait_until_accepted(
                    client,
                    escrow_state.txid,
                    escrow_address(BUYER_HASH, SELLER_HASH, ARBITER_HASH, refund_daa, 0),
                ),
            )
            show_step("genesis (locked)", escrow_state)
            open_addr = str(
                escrow_address(
                    BUYER_HASH, SELLER_HASH, ARBITER_HASH, refund_daa, escrow_state.status
                )
            )
            step = proof_step(
                "genesis", escrow_state.txid, escrow_state.covenant_id, escrow_state.status
            )
            step["open_address"] = open_addr
            steps.append(step)
            write_escrow_proof(PROOF_PATH, steps, funding_text, refund_daa)
            remaining = remaining[1:]

        if escrow_state is None:
            raise RuntimeError("no escrow state to continue (corrupt proof?)")

        if "release_seller" in remaining:
            current_daa = await rpc_virtual_daa(client)
            if current_daa >= refund_daa:
                print(
                    f"refund_daa passed (current {current_daa} >= {refund_daa}); "
                    "using timeout_to_buyer ..."
                )
                escrow_state = await timeout_to_buyer(client, escrow_state)
                steps.append(
                    proof_step(
                        "timeout_to_buyer",
                        escrow_state.txid,
                        escrow_state.covenant_id,
                        escrow_state.status,
                    )
                )
                write_escrow_proof(PROOF_PATH, steps, funding_text, refund_daa)
                print(f"Final status = {escrow_state.status}")
                if args.publish_fixture and len(steps) >= 2:
                    FIXTURE_PROOF.write_text(PROOF_PATH.read_text(encoding="utf-8"), encoding="utf-8")
                    print(f"published {FIXTURE_PROOF}")
                return
            try:
                escrow_state = await release_seller(client, escrow_state)
            except Exception as err:
                print(f"release_seller failed ({err}); trying timeout_to_buyer ...")
                escrow_state = await timeout_to_buyer(client, escrow_state)
                steps.append(
                    proof_step(
                        "timeout_to_buyer",
                        escrow_state.txid,
                        escrow_state.covenant_id,
                        escrow_state.status,
                    )
                )
                write_escrow_proof(PROOF_PATH, steps, funding_text, refund_daa)
                print(f"Final status = {escrow_state.status}")
                if args.publish_fixture and len(steps) >= 2:
                    FIXTURE_PROOF.write_text(PROOF_PATH.read_text(encoding="utf-8"), encoding="utf-8")
                    print(f"published {FIXTURE_PROOF}")
                return
            attach_live_utxo(
                escrow_state,
                await wait_until_accepted(
                    client,
                    escrow_state.txid,
                    escrow_address(
                        BUYER_HASH, SELLER_HASH, ARBITER_HASH, refund_daa, escrow_state.status
                    ),
                ),
            )
            show_step("release_seller (seller + arbiter)", escrow_state)
            steps.append(
                proof_step(
                    "release_seller",
                    escrow_state.txid,
                    escrow_state.covenant_id,
                    escrow_state.status,
                )
            )
            write_escrow_proof(PROOF_PATH, steps, funding_text, refund_daa)

        print(f"Final status = {escrow_state.status}")
        if args.publish_fixture and len(steps) == len(FLOW):
            FIXTURE_PROOF.write_text(PROOF_PATH.read_text(encoding="utf-8"), encoding="utf-8")
            print(f"published {FIXTURE_PROOF}")
    finally:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
