"""Deploy HtlcBridgeReleaseSha256 on Galleon (wiKAS vault for TN10 SHA256 HTLC).

  python examples/l1_l2_bridge_deploy_sha256.py --simulate
  python examples/l1_l2_bridge_deploy_sha256.py --broadcast

Requires GALLEON_PRIVATE_KEY. Writes GALLEON_HTLC_BRIDGE_SHA256 to kaspa.env on broadcast.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

from eth_account import Account

ROOT = Path(__file__).resolve().parents[1]
GALLEON_DIR = ROOT / "examples" / "galleon"
ARTIFACT = (
    GALLEON_DIR / "out" / "HtlcBridgeReleaseSha256.sol" / "HtlcBridgeReleaseSha256.json"
)

sys.path.insert(0, str(ROOT / "scripts"))
from galleon import (  # noqa: E402
    GALLEON_CHAIN_ID,
    GALLEON_EXPLORER,
    GALLEON_MIN_GAS_WEI,
    GALLEON_RPC,
    GALLEON_WRAPPED_IKAS,
    HTLC_BRIDGE_DEPLOY_IKAS,
    fits_balance,
)
from galleon_faucet import address_of, galleon_key, require_galleon_chain, rpc_hex  # noqa: E402
from kaspa_env import load_kaspa_env, upsert_kaspa_env  # noqa: E402

PAYMENT_HASH_HEX = "0x455c7c63032972f9519d91194e1c7facd79fec84e9fcf24ef540f02d85b6e2d4"
# 0.001 wiKAS — use integer literal (float 0.001 * 1e18 rounds wrong in some encoders).
PAYOUT_WEI = 10**15
MIN_DEPLOY_IKAS_WEI = int(HTLC_BRIDGE_DEPLOY_IKAS * 1e18)
FORGE_BROADCAST_TIMEOUT_S = 180
DEPLOY_GAS_HEADROOM = 1.12


def ensure_artifact() -> dict:
    if not ARTIFACT.is_file():
        subprocess.run(["forge", "build"], cwd=GALLEON_DIR, check=True)
    return json.loads(ARTIFACT.read_text(encoding="utf-8"))


def encode_constructor_args(wikas: str, payment_hash_hex: str, payout_wei: int) -> str:
    wikas_padded = wikas.lower().removeprefix("0x").zfill(64)
    hash_body = payment_hash_hex.lower().removeprefix("0x").zfill(64)
    payout_padded = f"{payout_wei:064x}"
    return wikas_padded + hash_body + payout_padded


def deploy_data(artifact: dict, wikas: str, payment_hash_hex: str, payout_wei: int) -> str:
    bytecode = artifact["bytecode"]["object"]
    if bytecode.startswith("0x"):
        bytecode = bytecode[2:]
    return "0x" + bytecode + encode_constructor_args(wikas, payment_hash_hex, payout_wei)


def rpc_json(method: str, params: list):
    payload = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    import urllib.request

    req = urllib.request.Request(
        GALLEON_RPC,
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=45) as resp:
        body = json.loads(resp.read().decode())
    if "error" in body:
        raise RuntimeError(str(body["error"]))
    return body.get("result")


def raw_broadcast(artifact: dict) -> str:
    """Contract-create tx; avoids forge script hanging on Galleon receipt polling."""
    require_galleon_chain()
    key = galleon_key()
    addr = address_of(key)
    data = deploy_data(artifact, GALLEON_WRAPPED_IKAS, PAYMENT_HASH_HEX, PAYOUT_WEI)
    bal = int(rpc_hex("eth_getBalance", [addr, "latest"]), 16)
    est = int(rpc_json("eth_estimateGas", [{"from": addr, "data": data}]), 16)
    gas = int(est * DEPLOY_GAS_HEADROOM) + 5000
    need = gas * GALLEON_MIN_GAS_WEI
    if not fits_balance(bal, gas, GALLEON_MIN_GAS_WEI):
        raise RuntimeError(
            f"need ~{need / 1e18:.4f} iKAS for deploy (est {est} gas @ {GALLEON_MIN_GAS_WEI} gwei); "
            f"have {bal / 1e18:.4f}"
        )
    nonce = int(rpc_hex("eth_getTransactionCount", [addr, "pending"]), 16)
    print(f"broadcasting HtlcBridgeReleaseSha256 (raw create, gas={gas}, est={est})...")
    tx = {
        "chainId": GALLEON_CHAIN_ID,
        "nonce": nonce,
        "value": 0,
        "gas": gas,
        "gasPrice": GALLEON_MIN_GAS_WEI,
        "data": data,
    }
    signed = Account.sign_transaction(tx, key)
    raw = signed.raw_transaction.hex()
    if not raw.startswith("0x"):
        raw = "0x" + raw
    tx_hash = rpc_hex("eth_sendRawTransaction", [raw])
    print(f"tx      {tx_hash}")
    print(f"        {GALLEON_EXPLORER}/tx/{tx_hash}")
    for attempt in range(120):
        rcpt = rpc_json("eth_getTransactionReceipt", [tx_hash])
        if rcpt:
            status = int(rcpt.get("status", "0x0"), 16)
            contract = (rcpt.get("contractAddress") or "").strip()
            gas_used = int(rcpt.get("gasUsed", "0x0"), 16)
            print(f"receipt status={status} gasUsed={gas_used} contract={contract}")
            if status != 1 or not contract:
                raise RuntimeError("deploy tx failed on-chain")
            payout = int(
                rpc_json(
                    "eth_call",
                    [
                        {
                            "to": contract,
                            "data": "0x6b46c8c3",  # payoutAmount()
                        },
                    ],
                ),
                16,
            )
            if payout != PAYOUT_WEI:
                raise RuntimeError(
                    f"deployed payout {payout} != expected {PAYOUT_WEI}; "
                    "do not save this address"
                )
            return contract
        if attempt % 6 == 5:
            print(f"waiting receipt... {(attempt + 1) * 5}s")
        time.sleep(5)
    raise RuntimeError(f"no receipt for {tx_hash} after 10 minutes")


def preflight_galleon_balance(artifact: dict) -> None:
    key = galleon_key()
    addr = address_of(key)
    bal = int(rpc_hex("eth_getBalance", [addr, "latest"]), 16)
    data = deploy_data(artifact, GALLEON_WRAPPED_IKAS, PAYMENT_HASH_HEX, PAYOUT_WEI)
    est = int(rpc_json("eth_estimateGas", [{"from": addr, "data": data}]), 16)
    gas = int(est * DEPLOY_GAS_HEADROOM) + 5000
    need = gas * GALLEON_MIN_GAS_WEI
    print(f"wallet  {addr}  balance {bal / 1e18:.4f} iKAS")
    print(f"deploy  est {est} gas  need ~{need / 1e18:.4f} iKAS @ {GALLEON_MIN_GAS_WEI} gwei")
    if not fits_balance(bal, gas, GALLEON_MIN_GAS_WEI):
        raise RuntimeError(
            f"need ~{need / 1e18:.4f} iKAS for raw deploy (have {bal / 1e18:.4f}). "
            "Run: python examples/galleon_faucet.py --fund-bridge"
        )


def forge_broadcast() -> str:
    key = galleon_key()
    if not key.startswith("0x"):
        key = f"0x{key}"
    print("broadcasting HtlcBridgeReleaseSha256 (forge script)...")
    cmd = [
        "forge",
        "script",
        "script/HtlcBridgeReleaseSha256.s.sol:HtlcBridgeReleaseSha256Script",
        "--rpc-url",
        GALLEON_RPC,
        "--broadcast",
        "--private-key",
        key,
        "--legacy",
        "--with-gas-price",
        str(GALLEON_MIN_GAS_WEI),
    ]
    proc = subprocess.run(
        cmd,
        cwd=GALLEON_DIR,
        capture_output=True,
        text=True,
        timeout=FORGE_BROADCAST_TIMEOUT_S,
    )
    if proc.returncode != 0:
        detail = proc.stderr.strip() or proc.stdout.strip() or "forge broadcast failed"
        raise RuntimeError(detail)
    if proc.stdout.strip():
        print(proc.stdout.strip())
    run_json = (
        GALLEON_DIR
        / "broadcast"
        / "HtlcBridgeReleaseSha256.s.sol"
        / str(GALLEON_CHAIN_ID)
        / "run-latest.json"
    )
    if not run_json.is_file():
        raise RuntimeError(f"missing broadcast artifact {run_json}")
    body = json.loads(run_json.read_text(encoding="utf-8"))
    for tx in body.get("transactions", []):
        addr = (tx.get("contractAddress") or "").strip()
        if addr:
            return addr
    raise RuntimeError("broadcast succeeded but no contractAddress in run-latest.json")


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Deploy HtlcBridgeReleaseSha256 on Galleon")
    parser.add_argument("--simulate", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    parser.add_argument(
        "--raw",
        action="store_true",
        help="contract-create broadcast (default; forge script often hangs on Galleon)",
    )
    parser.add_argument(
        "--forge",
        action="store_true",
        help="use forge script --broadcast instead of raw create",
    )
    args = parser.parse_args()

    artifact = ensure_artifact()
    print(f"chain   {GALLEON_CHAIN_ID}  rpc {GALLEON_RPC}", flush=True)
    print(f"wikas   {GALLEON_WRAPPED_IKAS}")
    print(f"hash    {PAYMENT_HASH_HEX}")
    print(f"payout  {PAYOUT_WEI} wei wiKAS")

    if args.simulate or not args.broadcast:
        subprocess.run(
            [
                "forge",
                "script",
                "script/HtlcBridgeReleaseSha256.s.sol:HtlcBridgeReleaseSha256Script",
                "--rpc-url",
                GALLEON_RPC,
            ],
            cwd=GALLEON_DIR,
            check=True,
        )
        print("simulate OK — use --broadcast when wallet has prepaid iKAS at 2000 gwei")
        return 0

    preflight_galleon_balance(artifact)
    if args.forge:
        bridge = forge_broadcast()
    else:
        bridge = raw_broadcast(artifact)
    print(f"bridge  {bridge}")
    print(f"        {GALLEON_EXPLORER}/address/{bridge}")
    upsert_kaspa_env({"GALLEON_HTLC_BRIDGE_SHA256": bridge}, ROOT)
    print("saved GALLEON_HTLC_BRIDGE_SHA256 in kaspa.env")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
