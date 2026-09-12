"""TN10 SilverScript HTLC: genesis + hashlock claim (refund path after refund_daa).

Demo uses int payment_hash (not SHA256) like restricted_swap recipient_hash.
Off-chain policy mirror: src/covenant.rs HtlcPolicyEngine.

  python examples/silverscript/htlc.py --print-address
  python examples/silverscript/htlc.py
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
    GRAMS_PER_COMPUTE_BUDGET_UNIT,
    LOCAL,
    NETWORK_ID,
    NETWORK_TYPE,
    ROOT,
    SUBNETWORK_ID,
    TX_VERSION,
    ensure_explorer_urls,
    funding_inputs,
    load_or_create_funder,
    load_proof_steps,
    load_proof_refund_daa,
    DEFAULT_GENESIS_LOCK_SOMPI,
    MIN_GENESIS_INPUT_SOMPI,
    genesis_lock_split,
    make_client,
    proof_step,
    remaining_flow,
    require_toccata_sdk,
    rest_address_utxos,
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
from tn10_wallets import WALLET_NAMES  # noqa: E402

# Demo preimage / hash (int tag, not SHA256). Production apps commit a real hash.
PAYMENT_PREIMAGE_HASH = 0x48544C43  # "HTLC"
PAYMENT_HASH = PAYMENT_PREIMAGE_HASH
FLOW_CLAIM = ("genesis", "claim")
FLOW_REFUND = ("genesis", "refund")
PROOF_PATH = LOCAL / "tn10-htlc-proof.json"
FIXTURE_PROOF = ROOT / "fixtures" / "tn10-htlc-proof.json"
REFUND_PROOF_PATH = LOCAL / "tn10-htlc-refund-proof.json"
FIXTURE_REFUND_PROOF = ROOT / "fixtures" / "tn10-htlc-refund-proof.json"
# ~24h claim window at TN10 10 BPS (10 blocks/s → 86_400 DAA ≈ 2.4h).
DAA_REFUND_OFFSET = 86_400
# Short timelock for refund rehearsal (~12s at 10 BPS).
REFUND_REHEARSAL_OFFSET = 120

SOURCE = """
pragma silverscript ^0.1.0;

contract Htlc(int payment_hash, int refund_daa) {
 int status = 0;

 #[covenant(binding = auth, from = 1, to = 1, mode = transition)]
 function claim(State prev_state, int preimage_hash, int current_daa) : (State) {
 require(prev_state.status == 0);
 require(preimage_hash == payment_hash);
 require(current_daa < refund_daa);
 return({ status: 1 });
 }

 #[covenant(binding = auth, from = 1, to = 1, mode = transition)]
 function refund(State prev_state, int current_daa) : (State) {
 require(prev_state.status == 0);
 require(current_daa >= refund_daa);
 return({ status: 2 });
 }
}
"""


@lru_cache(maxsize=8)
def compiled_htlc(payment_hash: int, refund_daa: int) -> silverscript.CompiledContract:
    return silverscript.compile(SOURCE, [payment_hash, refund_daa])


def lock_script(payment_hash: int, refund_daa: int, status: int) -> ScriptPublicKey:
    contract = compiled_htlc(payment_hash, refund_daa)
    redeem = script_with_int_state(contract, status)
    return ScriptBuilder.from_script(redeem, covenants_enabled=True).create_pay_to_script_hash_script()


def legacy_lock_script(payment_hash: int, refund_daa: int, status: int) -> ScriptPublicKey:
    """P2SH for genesis outputs locked before the int tag-byte fix."""
    contract = compiled_htlc(payment_hash, refund_daa)
    redeem = script_with_int_state_redeem(contract, status)
    return ScriptBuilder.from_script(redeem, covenants_enabled=True).create_pay_to_script_hash_script()


def htlc_address(payment_hash: int, refund_daa: int, status: int = 0) -> Address:
    return address_from_script_public_key(
        lock_script(payment_hash, refund_daa, status), NETWORK_TYPE
    )


def legacy_htlc_address(payment_hash: int, refund_daa: int, status: int = 0) -> Address:
    return address_from_script_public_key(
        legacy_lock_script(payment_hash, refund_daa, status), NETWORK_TYPE
    )


def legacy_status0_spend(state: HtlcState) -> bool:
    """Pre-fix genesis outputs on legacy_htlc_address use untagged int redeem bytes."""
    if state.status != 0 or not state.live_utxo:
        return False
    addr = state.live_utxo.get("address") or ""
    return addr == str(legacy_htlc_address(state.payment_hash, state.refund_daa, 0))


def unlock_script(
    payment_hash: int,
    refund_daa: int,
    status: int,
    function: str,
    preimage_hash: int,
    current_daa: int,
    *,
    legacy_redeem: bool = False,
) -> bytes:
    contract = compiled_htlc(payment_hash, refund_daa)
    if function == "claim":
        call = contract.build_sig_script_for_covenant_decl(
            "claim", [preimage_hash, current_daa]
        )
    elif function == "refund":
        call = contract.build_sig_script_for_covenant_decl("refund", [current_daa])
    else:
        raise ValueError(f"unknown HTLC function {function}")
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
class HtlcState:
    txid: str
    status: int
    value: int
    covenant_id: str
    payment_hash: int
    refund_daa: int
    live_utxo: dict | None = field(default=None, repr=False)

    @property
    def outpoint(self) -> TransactionOutpoint:
        return TransactionOutpoint(Hash(self.txid), 0)

    @property
    def utxo(self) -> UtxoEntryReference:
        spk = lock_script(self.payment_hash, self.refund_daa, self.status)
        addr = htlc_address(self.payment_hash, self.refund_daa, self.status)
        return UtxoEntryReference.from_dict({
            "address": addr.to_string(),
            "outpoint": {"transactionId": self.txid, "index": 0},
            "utxoEntry": {
                "amount": self.value,
                "scriptPublicKey": {"version": spk.version, "script": spk.script},
                "blockDaaScore": 0,
                "isCoinbase": False,
                "covenantId": self.covenant_id,
            },
        })


async def build_htlc_tx(
    client: RpcClient,
    spends: list[TransactionInput],
    value_in: int,
    payment_hash: int,
    refund_daa: int,
    status: int,
    covenant: CovenantBinding | None,
    *,
    max_lock_sompi: int | None = None,
    change_spk: ScriptPublicKey | None = None,
    min_lock_sompi: int = MIN_GENESIS_INPUT_SOMPI,
) -> tuple[Transaction, int]:
    spk = lock_script(payment_hash, refund_daa, status)
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
    payment_hash: int,
    refund_daa: int,
    min_input_sompi: int = MIN_GENESIS_INPUT_SOMPI,
    max_lock_sompi: int = DEFAULT_GENESIS_LOCK_SOMPI,
) -> HtlcState:
    selected = select_genesis_utxos(funding_utxos, min_input_sompi=min_input_sompi)
    spends = funding_inputs(selected)
    value_in = sum(utxo_amount(entry) for entry in selected)
    change_spk = pay_to_address_script(funder_key.to_public_key().to_address(NETWORK_TYPE))
    tx, value = await build_htlc_tx(
        client,
        spends,
        value_in,
        payment_hash,
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
    return HtlcState(
        result["transactionId"],
        status=0,
        value=value,
        covenant_id=covenant_id,
        payment_hash=payment_hash,
        refund_daa=refund_daa,
    )


async def claim(client: RpcClient, state: HtlcState) -> HtlcState:
    current_daa = await rpc_virtual_daa(client)
    if current_daa >= state.refund_daa:
        raise RuntimeError(
            f"refund window open: current DAA {current_daa} >= {state.refund_daa}"
        )
    spend_utxo = state.live_utxo if state.live_utxo else state.utxo
    spend = TransactionInput(
        state.outpoint,
        unlock_script(
            state.payment_hash,
            state.refund_daa,
            state.status,
            "claim",
            PAYMENT_PREIMAGE_HASH,
            current_daa,
            legacy_redeem=legacy_status0_spend(state),
        ),
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
        utxo=(
            spend_utxo
            if isinstance(spend_utxo, UtxoEntryReference)
            else UtxoEntryReference.from_dict(spend_utxo)
        ),
    )
    binding = CovenantBinding(authorizing_input=0, covenant_id=Hash(state.covenant_id))
    tx, value = await build_htlc_tx(
        client, [spend], state.value, state.payment_hash, state.refund_daa, 1, binding
    )
    result = await submit_transaction(client, {"transaction": tx, "allowOrphan": False})
    return HtlcState(
        result["transactionId"],
        status=1,
        value=value,
        covenant_id=state.covenant_id,
        payment_hash=state.payment_hash,
        refund_daa=state.refund_daa,
    )


async def refund(client: RpcClient, state: HtlcState) -> HtlcState:
    current_daa = await rpc_virtual_daa(client)
    if current_daa < state.refund_daa:
        raise RuntimeError(
            f"refund before timelock: need DAA {state.refund_daa}, current {current_daa}"
        )
    spend_utxo = state.live_utxo if state.live_utxo else state.utxo
    spend = TransactionInput(
        state.outpoint,
        unlock_script(
            state.payment_hash,
            state.refund_daa,
            state.status,
            "refund",
            PAYMENT_PREIMAGE_HASH,
            current_daa,
            legacy_redeem=legacy_status0_spend(state),
        ),
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
        utxo=(
            spend_utxo
            if isinstance(spend_utxo, UtxoEntryReference)
            else UtxoEntryReference.from_dict(spend_utxo)
        ),
    )
    binding = CovenantBinding(authorizing_input=0, covenant_id=Hash(state.covenant_id))
    tx, value = await build_htlc_tx(
        client, [spend], state.value, state.payment_hash, state.refund_daa, 2, binding
    )
    result = await submit_transaction(client, {"transaction": tx, "allowOrphan": False})
    return HtlcState(
        result["transactionId"],
        status=2,
        value=value,
        covenant_id=state.covenant_id,
        payment_hash=state.payment_hash,
        refund_daa=state.refund_daa,
    )


def attach_live_utxo(state: HtlcState, entry: dict) -> HtlcState:
    state.live_utxo = entry
    state.value = utxo_amount(entry)
    return state


def show_step(label: str, state: HtlcState) -> None:
    print(label)
    print(f"  status      {state.status}")
    print(f"  payment_hash {state.payment_hash}")
    print(f"  refund_daa  {state.refund_daa}")
    print(f"  address     {htlc_address(state.payment_hash, state.refund_daa, state.status)}")
    print(f"  covenant    {state.covenant_id}")
    print(f"  value       {state.value:,} sompi")
    print(f"  txid        {state.txid}")
    print(f"  explorer    {EXPLORER}/txs/{state.txid}")
    print()


async def recover_last(
    client: RpcClient, last: dict, payment_hash: int, refund_daa: int
) -> HtlcState:
    state = HtlcState(
        str(last["txid"]),
        int(last.get("count", 0)),
        0,
        str(last["covenant_id"]),
        payment_hash,
        refund_daa,
    )
    addresses = [legacy_htlc_address(payment_hash, refund_daa, state.status)]
    if state.status != 0:
        addresses.append(htlc_address(payment_hash, refund_daa, state.status))
    else:
        addresses.append(htlc_address(payment_hash, refund_daa, state.status))
    for addr in addresses:
        entries: list[dict] = []
        try:
            result = await client.get_utxos_by_addresses({"addresses": [addr]})
            entries = result.get("entries") or []
        except Exception:
            try:
                entries = rest_address_utxos(str(addr))
            except Exception:
                continue
        for entry in entries:
            if entry["outpoint"]["transactionId"] == state.txid:
                attach_live_utxo(state, entry)
                return state
    raise RuntimeError(f"proof txid {state.txid} not in UTXO set. {EXPLORER}/txs/{state.txid}")


def write_htlc_proof(
    path: Path,
    steps: list[dict],
    funding_address: str,
    payment_hash: int,
    refund_daa: int,
) -> None:
    LOCAL.mkdir(exist_ok=True)
    body = {
        "app": "htlc",
        "network": NETWORK_ID,
        "explorer": EXPLORER,
        "funding_address": funding_address,
        "payment_hash": payment_hash,
        "refund_daa": refund_daa,
        "steps": steps,
    }
    path.write_text(json.dumps(body, indent=2), encoding="utf-8")
    print(f"wrote {path}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="TN10 SilverScript HTLC reference")
    parser.add_argument("--print-address", action="store_true")
    parser.add_argument("--print-source", action="store_true")
    parser.add_argument("--no-resume", action="store_true")
    parser.add_argument("--publish-fixture", action="store_true")
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
    parser.add_argument(
        "--wallet",
        choices=WALLET_NAMES,
        help="named TN10 wallet for genesis funding (default: KASPA_TN10_FUNDING_KEY / alice)",
    )
    parser.add_argument(
        "--refund-rehearsal",
        action="store_true",
        help="genesis + refund with short timelock; writes tn10-htlc-refund-proof.json",
    )
    parser.add_argument(
        "--funds-timeout",
        type=int,
        default=None,
        help="seconds to wait for genesis funding (default 45m; refund-rehearsal uses 120s)",
    )
    return parser.parse_args()


async def wait_until_refund_daa(client: RpcClient, refund_daa: int) -> int:
    while True:
        current = await rpc_virtual_daa(client)
        if current >= refund_daa:
            return current
        print(f"waiting for refund_daa {refund_daa} (current {current}) ...")
        await asyncio.sleep(2)


def load_funder(wallet: str | None) -> tuple[PrivateKey, Address]:
    if wallet:
        sys.path.insert(0, str(ROOT / "scripts"))
        from tn10_wallets import get_wallet, key_for

        key = key_for(wallet)
        addr = Address(get_wallet(wallet).address)
        return key, addr
    return load_or_create_funder()


async def main() -> None:
    args = parse_args()
    if args.print_source:
        print(SOURCE.strip())
        return

    flow = FLOW_REFUND if args.refund_rehearsal else FLOW_CLAIM
    proof_path = REFUND_PROOF_PATH if args.refund_rehearsal else PROOF_PATH
    fixture_path = FIXTURE_REFUND_PROOF if args.refund_rehearsal else FIXTURE_PROOF
    daa_offset = REFUND_REHEARSAL_OFFSET if args.refund_rehearsal else DAA_REFUND_OFFSET

    LOCAL.mkdir(exist_ok=True)
    funder_key, funding_address = load_funder(args.wallet)
    funding_text = str(funding_address)
    if not funding_text.startswith("kaspatest:"):
        raise RuntimeError(f"TN10 HTLC refuses non-testnet funding address: {funding_text}")

    if args.no_resume:
        refund_daa = virtual_daa() + daa_offset
    else:
        refund_daa = load_proof_refund_daa(proof_path)
        if refund_daa is None:
            refund_daa = virtual_daa() + daa_offset

    print("Fund this address with at least 1 tKAS (testnet only):")
    print(f"  {funding_address}")
    print(f"Faucet: {FAUCET}")
    print(f"HTLC open address (status=0): {htlc_address(PAYMENT_HASH, refund_daa, 0)}\n")
    if args.print_address:
        return

    require_toccata_sdk()
    steps = [] if args.no_resume else load_proof_steps(proof_path)
    remaining = remaining_flow(steps, flow)
    if not remaining:
        steps, changed = ensure_explorer_urls(steps)
        if changed:
            write_htlc_proof(proof_path, steps, funding_text, PAYMENT_HASH, refund_daa)
        print("Proof already complete:")
        for step in steps:
            print(f"  {step.get('step')}  {step.get('explorer') or step.get('txid')}")
        if args.publish_fixture:
            fixture_path.write_text(proof_path.read_text(encoding="utf-8"), encoding="utf-8")
            print(f"published {fixture_path}")
        return

    print(
        f"Htlc on {NETWORK_ID}  branch={flow[-1]}  payment_hash={PAYMENT_HASH}  "
        f"refund_daa={refund_daa}  remaining={list(remaining)}\n"
    )
    client = await make_client()
    print("connected\n")
    try:
        htlc: HtlcState | None = None
        if steps:
            htlc = await recover_last(client, steps[-1], PAYMENT_HASH, refund_daa)
            show_step(f"recovered status={htlc.status}", htlc)

        if remaining[0] == "genesis":
            funds_timeout = args.funds_timeout
            if funds_timeout is None:
                funds_timeout = 120 if args.refund_rehearsal else None
            if funds_timeout is None:
                from covenant_common import FUNDS_TIMEOUT_S

                funds_timeout = FUNDS_TIMEOUT_S
            funding_utxos = await wait_for_funds(
                client,
                funding_address,
                min_funding_sompi=args.min_funding_sompi,
                timeout_s=funds_timeout,
            )
            htlc = await genesis(
                client,
                funder_key,
                funding_utxos,
                PAYMENT_HASH,
                refund_daa,
                min_input_sompi=args.min_funding_sompi,
                max_lock_sompi=args.max_lock_sompi,
            )
            attach_live_utxo(
                htlc,
                await wait_until_accepted(
                    client, htlc.txid, htlc_address(PAYMENT_HASH, refund_daa, 0)
                ),
            )
            show_step("genesis (locked)", htlc)
            steps.append(proof_step("genesis", htlc.txid, htlc.covenant_id, htlc.status))
            write_htlc_proof(proof_path, steps, funding_text, PAYMENT_HASH, refund_daa)
            remaining = remaining[1:]

        if htlc is None:
            raise RuntimeError("no HTLC state to continue (corrupt proof?)")

        if "claim" in remaining:
            try:
                htlc = await claim(client, htlc)
                step_name = "claim"
                label = "claim (hashlock satisfied)"
            except RuntimeError as err:
                if "refund window open" not in str(err):
                    raise
                print(f"claim window closed ({err}); taking refund path")
                htlc = await refund(client, htlc)
                step_name = "refund"
                label = "refund (timelock expired)"
            attach_live_utxo(
                htlc,
                await wait_until_accepted(
                    client, htlc.txid, htlc_address(PAYMENT_HASH, refund_daa, htlc.status)
                ),
            )
            show_step(label, htlc)
            steps.append(proof_step(step_name, htlc.txid, htlc.covenant_id, htlc.status))
            write_htlc_proof(proof_path, steps, funding_text, PAYMENT_HASH, refund_daa)

        if "refund" in remaining:
            await wait_until_refund_daa(client, refund_daa)
            htlc = await refund(client, htlc)
            attach_live_utxo(
                htlc,
                await wait_until_accepted(
                    client, htlc.txid, htlc_address(PAYMENT_HASH, refund_daa, htlc.status)
                ),
            )
            show_step("refund (timelock expired)", htlc)
            steps.append(proof_step("refund", htlc.txid, htlc.covenant_id, htlc.status))
            write_htlc_proof(proof_path, steps, funding_text, PAYMENT_HASH, refund_daa)

        print(f"Final status = {htlc.status}")
        if args.publish_fixture and len(steps) == len(flow):
            fixture_path.write_text(proof_path.read_text(encoding="utf-8"), encoding="utf-8")
            print(f"published {fixture_path}")
    finally:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
