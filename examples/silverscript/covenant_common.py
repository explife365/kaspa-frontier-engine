"""Shared TN10 SilverScript covenant helpers (RPC, proof I/O, SDK gate)."""

from __future__ import annotations

import asyncio
import json
import os
import sys
import time
from pathlib import Path

from kaspa import (
    Address,
    Hash,
    Keypair,
    PrivateKey,
    Resolver,
    RpcClient,
    TransactionInput,
    TransactionOutpoint,
    UtxoEntryReference,
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
MIN_GENESIS_UTXO_SOMPI = 10_000_000
MIN_GENESIS_INPUT_SOMPI = 50_000_000
# Cap covenant genesis lock so a large faucet drip keeps change on the funder.
DEFAULT_GENESIS_LOCK_SOMPI = 1_000_000_000
CHANGE_FLOOR_SOMPI = 20_000_000
FUNDS_TIMEOUT_S = 45 * 60
ACCEPT_TIMEOUT_S = 180
SUBMIT_RETRIES = 3
EXPLORER = "https://explorer-tn10.kaspa.org"
FAUCET = "https://faucet-tn10.kaspanet.io/"

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from kaspa_env import load_kaspa_env, upsert_kaspa_env  # noqa: E402
from tn10_rest import is_mature_utxo, virtual_daa, address_utxos as rest_address_utxos, fee_estimate as rest_fee_estimate  # noqa: E402

LOCAL = ROOT / ".local"
load_kaspa_env(ROOT)
RPC_URL = (os.environ.get("KASPA_RPC_URL") or "").strip() or None

from kaspa_sdk_dev_patch import (  # noqa: E402
    dev_patch_enabled,
    ensure_dev_patch_if_enabled,
)

ensure_dev_patch_if_enabled()


def script_with_bool_state(contract: silverscript.CompiledContract, flag: bool) -> bytes:
    """Patch bool covenant state embedded in a compiled SilverScript script."""
    script = bytearray(contract.script)
    start, length = contract.state_layout
    if length >= 2:
        script[start + 1] = 1 if flag else 0
    return bytes(script)


def script_with_int_state(contract: silverscript.CompiledContract, value: int) -> bytes:
    """Patch int covenant state embedded in a compiled SilverScript script."""
    script = bytearray(contract.script)
    start, length = contract.state_layout
    encoded = int(value).to_bytes(8, "little", signed=True)
    if length == 9:
        # SilverScript tagged int: 1-byte tag + 8-byte little-endian value.
        script[start + 1 : start + 9] = encoded
    elif length >= 8:
        script[start : start + 8] = encoded
    elif length >= 4:
        script[start : start + 4] = int(value).to_bytes(4, "little", signed=True)
    return bytes(script)


def script_with_int_state_redeem(
    contract: silverscript.CompiledContract, value: int
) -> bytes:
    """Redeem bytes for spending a P2SH input (may differ from lock encoding)."""
    script = bytearray(contract.script)
    start, length = contract.state_layout
    if length == 9 and value == 0:
        # Pre-fix genesis outputs cleared the tag byte; match that for spends.
        script[start : start + 8] = int(value).to_bytes(8, "little", signed=True)
        return bytes(script)
    return script_with_int_state(contract, value)


async def rpc_priority_feerate(client: RpcClient) -> int:
    try:
        estimate = await client.get_fee_estimate()
        return int(estimate["estimate"]["priorityBucket"]["feerate"])
    except Exception:
        estimate = rest_fee_estimate()
        if estimate and isinstance(estimate.get("estimate"), dict):
            bucket = estimate["estimate"].get("priorityBucket") or {}
            return int(bucket.get("feerate") or 100)
        return 100


async def rpc_virtual_daa(client: RpcClient) -> int:
    try:
        info = await client.get_block_dag_info()
        for key in ("virtualDaaScore", "virtualSelectedParentDaaScore"):
            value = info.get(key)
            if value is not None:
                return int(value)
    except Exception:
        pass
    return virtual_daa()


def require_toccata_sdk() -> None:
    ensure_dev_patch_if_enabled()
    probe = TransactionInput(
        TransactionOutpoint(Hash("00" * 32), 0),
        b"",
        sequence=0,
        sig_op_count=0,
        compute_budget=COMPUTE_BUDGET,
    )
    encoded = probe.to_dict()
    if encoded.get("computeBudget") != COMPUTE_BUDGET:
        dev_hint = (
            " Set TN10_SDK_DEV_PATCH=1 for TN10 testnet rehearsal only "
            "(see scripts/kaspa_sdk_dev_patch.py) until PR #78 publishes."
            if not dev_patch_enabled()
            else ""
        )
        raise RuntimeError(
            "installed kaspa-python-sdk drops computeBudget during serialization; "
            "refusing to fund or broadcast a broken Toccata v1 transaction. "
            "Use a published wheel that includes kaspa-python-sdk#78, then rerun."
            + dev_hint
        )


async def make_client() -> RpcClient:
    if RPC_URL:
        client = RpcClient(url=RPC_URL, network_id=NETWORK_ID)
        print(f"RPC {RPC_URL}")
    else:
        client = RpcClient(resolver=Resolver(), network_id=NETWORK_ID)
        print("RPC Resolver (public TN10)")
    await client.connect(strategy="fallback")
    return client


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


def genesis_lock_split(
    value_in: int,
    fee: int,
    min_lock_sompi: int,
    max_lock_sompi: int,
    change_floor_sompi: int = CHANGE_FLOOR_SOMPI,
) -> tuple[int, int]:
    """Split genesis input into covenant lock + optional funder change."""
    available = value_in - fee
    if available < min_lock_sompi:
        raise RuntimeError(
            f"genesis needs {min_lock_sompi} sompi after fees; have {available} "
            f"(in={value_in} fee={fee})"
        )
    lock = min(max_lock_sompi, available)
    change = available - lock
    if change > 0 and change < change_floor_sompi:
        lock = available
        change = 0
    return lock, change


def select_genesis_utxos(
    funding_utxos: list[dict], min_input_sompi: int = MIN_GENESIS_INPUT_SOMPI
) -> list[dict]:
    """Pick one large UTXO or combine inputs until covenant genesis fees fit."""
    ordered = sorted(funding_utxos, key=utxo_amount, reverse=True)
    if not ordered:
        return []
    if utxo_amount(ordered[0]) >= min_input_sompi:
        return [ordered[0]]
    selected: list[dict] = []
    total = 0
    for entry in ordered:
        selected.append(entry)
        total += utxo_amount(entry)
        if total >= min_input_sompi:
            return selected
    return selected


def funding_inputs(funder_entries: list[dict]) -> list[TransactionInput]:
    return [
        TransactionInput(
            TransactionOutpoint(
                Hash(entry["outpoint"]["transactionId"]),
                entry["outpoint"]["index"],
            ),
            b"",
            sequence=0,
            sig_op_count=0,
            compute_budget=COMPUTE_BUDGET,
            utxo=UtxoEntryReference.from_dict(entry),
        )
        for entry in funder_entries
    ]


async def wait_for_funds(
    client: RpcClient,
    addr: Address,
    min_funding_sompi: int = MIN_FUNDING_SOMPI,
    timeout_s: int = FUNDS_TIMEOUT_S,
) -> list[dict]:
    deadline = time.monotonic() + timeout_s
    while True:
        result = await client.get_utxos_by_addresses({"addresses": [addr]})
        daa = virtual_daa()
        mature = [
            e
            for e in result["entries"]
            if is_mature_utxo(e, daa) and utxo_amount(e) >= MIN_GENESIS_UTXO_SOMPI
        ]
        total = sum(utxo_amount(e) for e in mature)
        if mature and (
            max(utxo_amount(e) for e in mature) >= min_funding_sompi
            or total >= min_funding_sompi
        ):
            return mature
        if time.monotonic() >= deadline:
            raise TimeoutError(
                f"no mature spendable UTXO (need >= {MIN_GENESIS_UTXO_SOMPI} sompi each "
                f"and {min_funding_sompi} sompi total) after {timeout_s}s for {addr}"
            )
        immature = [
            e
            for e in result["entries"]
            if utxo_amount(e) >= MIN_GENESIS_UTXO_SOMPI and not is_mature_utxo(e, daa)
        ]
        if immature:
            print(f"waiting for coinbase maturity (1000 DAA) at {addr} ...")
        elif mature:
            print(
                f"waiting for more funds at {addr} "
                f"(have {total} sompi, need {min_funding_sompi}) ..."
            )
        else:
            print(
                f"waiting for faucet (>= {MIN_GENESIS_UTXO_SOMPI} sompi per UTXO) to {addr} ..."
            )
        await asyncio.sleep(2)


async def wait_until_accepted(client: RpcClient, txid: str, addr: Address) -> dict:
    deadline = time.monotonic() + ACCEPT_TIMEOUT_S
    while True:
        result = await client.get_utxos_by_addresses({"addresses": [addr]})
        for entry in result["entries"]:
            if entry["outpoint"]["transactionId"] == txid:
                return entry
        if time.monotonic() >= deadline:
            raise TimeoutError(f"txid {txid} not in UTXO set after {ACCEPT_TIMEOUT_S}s")
        await asyncio.sleep(1)


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


def proof_step(name: str, txid: str, covenant_id: str, state: int) -> dict:
    return {
        "step": name,
        "count": state,
        "txid": txid,
        "covenant_id": covenant_id,
        "output_index": 0,
        "explorer": f"{EXPLORER}/txs/{txid}",
    }


def write_proof(
    path: Path,
    app: str,
    steps: list[dict],
    funding_address: str,
    unlock_daa: int | None = None,
) -> None:
    LOCAL.mkdir(exist_ok=True)
    body: dict = {
        "app": app,
        "network": NETWORK_ID,
        "explorer": EXPLORER,
        "funding_address": funding_address,
        "steps": steps,
    }
    if unlock_daa is not None:
        body["unlock_daa"] = unlock_daa
    path.write_text(json.dumps(body, indent=2), encoding="utf-8")
    print(f"wrote {path}")


def load_proof_steps(path: Path) -> list[dict]:
    if not path.is_file():
        return []
    body = json.loads(path.read_text(encoding="utf-8"))
    steps = body.get("steps") or []
    if not isinstance(steps, list):
        return []
    return steps


def load_proof_unlock_daa(path: Path) -> int | None:
    if not path.is_file():
        return None
    body = json.loads(path.read_text(encoding="utf-8"))
    value = body.get("unlock_daa")
    return int(value) if value is not None else None


def load_proof_refund_daa(path: Path) -> int | None:
    if not path.is_file():
        return None
    body = json.loads(path.read_text(encoding="utf-8"))
    value = body.get("refund_daa")
    return int(value) if value is not None else None


def remaining_flow(steps: list[dict], flow: tuple[str, ...]) -> tuple[str, ...]:
    names = [str(s.get("step", "")) for s in steps]
    for i, expected in enumerate(flow):
        if i >= len(names):
            return flow[i:]
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
