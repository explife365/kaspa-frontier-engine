"""TN10 SilverScript timelock vault: genesis + DAA-gated release.

Broadcast path mirrors counter.py. Writes .local/tn10-vault-proof.json and can be
copied to fixtures/ after SDK #78 ships.

  python examples/silverscript/timelock_vault.py --print-address
  python examples/silverscript/timelock_vault.py

Off-chain destination policy: src/covenant.rs (not enforced in this contract).
"""

from __future__ import annotations

import argparse
import asyncio
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
    ACCEPT_TIMEOUT_S,
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
    load_proof_unlock_daa,
    make_client,
    proof_step,
    remaining_flow,
    require_toccata_sdk,
    submit_transaction,
    utxo_amount,
    virtual_daa,
    wait_for_funds,
    wait_until_accepted,
    write_proof,
)

FLOW = ("genesis", "release")
PROOF_PATH = LOCAL / "tn10-vault-proof.json"
FIXTURE_PROOF = ROOT / "fixtures" / "tn10-vault-proof.json"
DAA_UNLOCK_OFFSET = 120

SOURCE = """
pragma silverscript ^0.1.0;

contract TimelockVault(int unlock_daa) {
 bool released = false;

 #[covenant(binding = auth, from = 1, to = 1, mode = transition)]
 function release(State prev_state, int current_daa) : (State) {
 require(!prev_state.released);
 require(current_daa >= unlock_daa);
 return({ released: true });
 }
}
"""


@lru_cache(maxsize=8)
def compiled_vault(unlock_daa: int) -> silverscript.CompiledContract:
    return silverscript.compile(SOURCE, [unlock_daa])


def lock_script(unlock_daa: int, released: bool) -> ScriptPublicKey:
    redeem = compiled_vault(unlock_daa).script
    state = 1 if released else 0
    return ScriptBuilder.from_script(redeem, covenants_enabled=True).create_pay_to_script_hash_script()


def vault_address(unlock_daa: int, released: bool = False) -> Address:
    return address_from_script_public_key(lock_script(unlock_daa, released), NETWORK_TYPE)


def unlock_script(unlock_daa: int, released: bool, current_daa: int) -> bytes:
    contract = compiled_vault(unlock_daa)
    call = contract.build_sig_script_for_covenant_decl("release", [current_daa])
    redeem = bytes.fromhex(
        ScriptBuilder(covenants_enabled=True).add_data(contract.script).to_string()
    )
    return call + redeem


@dataclass
class Vault:
    txid: str
    released: bool
    value: int
    covenant_id: str
    unlock_daa: int
    live_utxo: dict | None = field(default=None, repr=False)

    @property
    def state(self) -> int:
        return 1 if self.released else 0

    @property
    def outpoint(self) -> TransactionOutpoint:
        return TransactionOutpoint(Hash(self.txid), 0)

    @property
    def utxo(self) -> UtxoEntryReference:
        spk = lock_script(self.unlock_daa, self.released)
        return UtxoEntryReference.from_dict({
            "address": vault_address(self.unlock_daa, self.released).to_string(),
            "outpoint": {"transactionId": self.txid, "index": 0},
            "utxoEntry": {
                "amount": self.value,
                "scriptPublicKey": {"version": spk.version, "script": spk.script},
                "blockDaaScore": 0,
                "isCoinbase": False,
                "covenantId": self.covenant_id,
            },
        })


async def build_vault_tx(
    client: RpcClient,
    spend: TransactionInput,
    value_in: int,
    unlock_daa: int,
    released: bool,
    covenant: CovenantBinding | None,
) -> tuple[Transaction, int]:
    spk = lock_script(unlock_daa, released)
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
    unlock_daa: int,
) -> Vault:
    funding = max(funding_utxos, key=utxo_amount)
    spend = TransactionInput(
        TransactionOutpoint(Hash(funding["outpoint"]["transactionId"]), funding["outpoint"]["index"]),
        b"",
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
        utxo=UtxoEntryReference.from_dict(funding),
    )
    tx, value = await build_vault_tx(client, spend, utxo_amount(funding), unlock_daa, False, None)
    tx.populate_genesis_covenants([GenesisCovenantGroup(authorizing_input=0, outputs=[0])])
    covenant_id = tx.outputs[0].to_dict()["covenant"]["covenantId"]
    signed = sign_transaction(tx, [funder_key], True)
    result = await submit_transaction(client, {"transaction": signed, "allowOrphan": False})
    return Vault(result["transactionId"], released=False, value=value, covenant_id=covenant_id, unlock_daa=unlock_daa)


async def release(client: RpcClient, vault: Vault) -> Vault:
    current_daa = virtual_daa()
    if current_daa < vault.unlock_daa:
        raise RuntimeError(
            f"DAA timelock active: need {vault.unlock_daa}, current {current_daa}"
        )
    spend_utxo = vault.live_utxo if vault.live_utxo else vault.utxo
    spend = TransactionInput(
        vault.outpoint,
        unlock_script(vault.unlock_daa, vault.released, current_daa),
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
        utxo=spend_utxo if isinstance(spend_utxo, UtxoEntryReference) else UtxoEntryReference.from_dict(spend_utxo),
    )
    binding = CovenantBinding(authorizing_input=0, covenant_id=Hash(vault.covenant_id))
    tx, value = await build_vault_tx(
        client, spend, vault.value, vault.unlock_daa, True, binding
    )
    result = await submit_transaction(client, {"transaction": tx, "allowOrphan": False})
    return Vault(
        result["transactionId"],
        released=True,
        value=value,
        covenant_id=vault.covenant_id,
        unlock_daa=vault.unlock_daa,
    )


def attach_live_utxo(vault: Vault, entry: dict) -> Vault:
    vault.live_utxo = entry
    vault.value = utxo_amount(entry)
    return vault


def show_step(label: str, vault: Vault) -> None:
    print(label)
    print(f"  released  {vault.released}")
    print(f"  unlock    DAA {vault.unlock_daa}")
    print(f"  address   {vault_address(vault.unlock_daa, vault.released)}")
    print(f"  covenant  {vault.covenant_id}")
    print(f"  value     {vault.value:,} sompi")
    print(f"  txid      {vault.txid}")
    print(f"  explorer  {EXPLORER}/txs/{vault.txid}")
    print()


async def wait_for_unlock(unlock_daa: int) -> None:
    deadline = time.monotonic() + ACCEPT_TIMEOUT_S
    while True:
        current = virtual_daa()
        if current >= unlock_daa:
            print(f"DAA unlock reached ({current} >= {unlock_daa})")
            return
        if time.monotonic() >= deadline:
            raise TimeoutError(f"unlock DAA {unlock_daa} not reached within {ACCEPT_TIMEOUT_S}s")
        print(f"waiting for DAA unlock ({current} < {unlock_daa}) ...")
        await asyncio.sleep(2)


async def recover_last(client: RpcClient, last: dict, unlock_daa: int) -> Vault:
    vault = Vault(
        str(last["txid"]),
        bool(int(last.get("count", 0))),
        0,
        str(last["covenant_id"]),
        unlock_daa,
    )
    addr = vault_address(unlock_daa, vault.released)
    result = await client.get_utxos_by_addresses({"addresses": [addr]})
    for entry in result["entries"]:
        if entry["outpoint"]["transactionId"] == vault.txid:
            attach_live_utxo(vault, entry)
            return vault
    raise RuntimeError(
        f"proof txid {vault.txid} is not in the UTXO set. {EXPLORER}/txs/{vault.txid}"
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="TN10 SilverScript timelock vault covenant")
    parser.add_argument("--print-address", action="store_true")
    parser.add_argument("--print-source", action="store_true")
    parser.add_argument("--no-resume", action="store_true")
    parser.add_argument(
        "--publish-fixture",
        action="store_true",
        help="copy completed .local proof to fixtures/tn10-vault-proof.json",
    )
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
        raise RuntimeError(f"TN10 vault refuses non-testnet funding address: {funding_text}")

    print("Fund this address with at least 1 tKAS (testnet only):")
    print(f"  {funding_address}")
    print(f"Faucet: {FAUCET}")
    print("Recovery key saved in kaspa.env (gitignored, testnet only)\n")
    if args.print_address:
        return

    require_toccata_sdk()
    steps = [] if args.no_resume else load_proof_steps(PROOF_PATH)
    unlock_daa = load_proof_unlock_daa(PROOF_PATH)
    if unlock_daa is None:
        unlock_daa = virtual_daa() + DAA_UNLOCK_OFFSET

    remaining = remaining_flow(steps, FLOW)
    if not remaining:
        steps, changed = ensure_explorer_urls(steps)
        if changed:
            write_proof(PROOF_PATH, "timelock_vault", steps, funding_text, unlock_daa)
        print("Proof already complete:")
        for step in steps:
            print(f"  {step.get('step')}  {step.get('explorer') or step.get('txid')}")
        if args.publish_fixture:
            FIXTURE_PROOF.write_text(PROOF_PATH.read_text(encoding="utf-8"), encoding="utf-8")
            print(f"published {FIXTURE_PROOF}")
        return

    print(f"TimelockVault on {NETWORK_ID}  unlock_daa={unlock_daa}  remaining={list(remaining)}\n")
    client = await make_client()
    print("connected\n")
    try:
        vault: Vault | None = None
        if steps:
            print(f"resuming after {steps[-1].get('step')} {steps[-1].get('txid')}")
            vault = await recover_last(client, steps[-1], unlock_daa)
            show_step(f"recovered released={vault.released}", vault)

        if remaining[0] == "genesis":
            funding_utxos = await wait_for_funds(client, funding_address)
            vault = await genesis(client, funder_key, funding_utxos, unlock_daa)
            attach_live_utxo(
                vault,
                await wait_until_accepted(client, vault.txid, vault_address(unlock_daa, False)),
            )
            show_step("genesis (locked)", vault)
            steps.append(proof_step("genesis", vault.txid, vault.covenant_id, vault.state))
            write_proof(PROOF_PATH, "timelock_vault", steps, funding_text, unlock_daa)
            remaining = remaining[1:]

        if vault is None:
            raise RuntimeError("no vault state to continue (corrupt proof?)")

        if "release" in remaining:
            await wait_for_unlock(vault.unlock_daa)
            vault = await release(client, vault)
            attach_live_utxo(
                vault,
                await wait_until_accepted(client, vault.txid, vault_address(unlock_daa, True)),
            )
            show_step("release (unlocked)", vault)
            steps.append(proof_step("release", vault.txid, vault.covenant_id, vault.state))
            write_proof(PROOF_PATH, "timelock_vault", steps, funding_text, unlock_daa)

        print(f"Final released = {vault.released}")
        print(f"Explorer: {EXPLORER}/txs/{vault.txid}")
        if args.publish_fixture and len(steps) == len(FLOW):
            FIXTURE_PROOF.write_text(PROOF_PATH.read_text(encoding="utf-8"), encoding="utf-8")
            print(f"published {FIXTURE_PROOF}")
    finally:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
