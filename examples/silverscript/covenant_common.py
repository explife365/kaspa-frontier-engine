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
)

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

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from kaspa_env import load_kaspa_env, upsert_kaspa_env  # noqa: E402
from tn10_rest import is_mature_utxo, virtual_daa  # noqa: E402

LOCAL = ROOT / ".local"
load_kaspa_env(ROOT)
RPC_URL = (os.environ.get("KASPA_RPC_URL") or "").strip() or None


def require_toccata_sdk() -> None:
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
