"""Fund and claim HtlcBridgeRelease on Galleon (L2 leg of L1 HTLC bridge rehearsal).

  python examples/l1_l2_bridge_release.py --status
  python examples/l1_l2_bridge_release.py --fund --dry-run
  python examples/l1_l2_bridge_release.py --fund --broadcast
  python examples/l1_l2_bridge_release.py --claim --broadcast

Requires GALLEON_HTLC_BRIDGE (deploy via l1_l2_bridge_deploy.py) and GALLEON_PRIVATE_KEY.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "examples" / "silverscript"))

from erc20 import decode_uint256, eth_call, token_balance, token_meta  # noqa: E402
from galleon import GALLEON_EXPLORER, GALLEON_RPC, GALLEON_WRAPPED_IKAS  # noqa: E402
from galleon_faucet import address_of, galleon_key, require_galleon_chain  # noqa: E402
from galleon_pool_seed import allowance_of, encode_approve, send_contract  # noqa: E402
from htlc import PAYMENT_PREIMAGE_HASH  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402

SELECTOR_CLAIM_DEMO = "0xee667c7b"
SELECTOR_DEPOSIT = "0xb6b55f25"
SELECTOR_CLAIMED = "0xe834a834"
SELECTOR_PAYMENT_HASH = "0x32e8542d"
SELECTOR_PAYOUT = "0x6b46c8c3"
SELECTOR_VAULT_BALANCE = "0x0bf6cc08"
APPROVE_GAS = 55_000
DEPOSIT_GAS = 65_000
CLAIM_GAS = 65_000
WRAP_GAS = 50_000
DEFAULT_FUND_WEI = int(0.001 * 1e18)


def _pad_uint(value: int) -> str:
    return f"{value:064x}"


def bridge_address() -> str:
    addr = (os.environ.get("GALLEON_HTLC_BRIDGE") or "").strip()
    if not addr:
        raise RuntimeError(
            "missing GALLEON_HTLC_BRIDGE; run python examples/l1_l2_bridge_deploy.py --broadcast"
        )
    return addr


def encode_claim_demo(preimage: int) -> str:
    return SELECTOR_CLAIM_DEMO + _pad_uint(preimage)


def encode_deposit_wikas(amount: int) -> str:
    return SELECTOR_DEPOSIT + _pad_uint(amount)


def read_bool(rpc: str, bridge: str, selector: str) -> bool:
    raw = eth_call(rpc, bridge, selector)
    return decode_uint256(raw) != 0


def read_uint(rpc: str, bridge: str, selector: str) -> int:
    return decode_uint256(eth_call(rpc, bridge, selector))


def status(rpc: str, bridge: str) -> dict:
    wikas_meta = token_meta(rpc, GALLEON_WRAPPED_IKAS)
    return {
        "bridge": bridge,
        "explorer": f"{GALLEON_EXPLORER}/address/{bridge}",
        "wikas": GALLEON_WRAPPED_IKAS,
        "claimed": read_bool(rpc, bridge, SELECTOR_CLAIMED),
        "payment_hash_int": read_uint(rpc, bridge, SELECTOR_PAYMENT_HASH),
        "payout_amount": read_uint(rpc, bridge, SELECTOR_PAYOUT),
        "vault_balance": read_uint(rpc, bridge, SELECTOR_VAULT_BALANCE),
        "payout_human": read_uint(rpc, bridge, SELECTOR_PAYOUT) / (10 ** wikas_meta["decimals"]),
    }


def fund_vault(rpc: str, bridge: str, amount: int, broadcast: bool) -> None:
    require_galleon_chain()
    key = galleon_key()
    owner = address_of(key)
    payout = read_uint(rpc, bridge, SELECTOR_PAYOUT)
    if amount < payout:
        raise RuntimeError(f"fund amount {amount} < contract payout {payout}")
    bal = token_balance(rpc, GALLEON_WRAPPED_IKAS, owner)
    if bal < amount:
        raise RuntimeError(
            f"insufficient wiKAS: have {bal}, need {amount}. "
            "Wrap iKAS via examples/galleon_faucet.py / WrappedIkas deposit."
        )
    if not broadcast:
        print(f"dry-run fund {amount} wiKAS -> {bridge}")
        return
    if allowance_of(GALLEON_WRAPPED_IKAS, owner, bridge) < amount:
        tx = send_contract(key, GALLEON_WRAPPED_IKAS, encode_approve(bridge, amount), APPROVE_GAS)
        print(f"approve {tx}")
    tx = send_contract(key, bridge, encode_deposit_wikas(amount), DEPOSIT_GAS)
    print(f"deposit {tx}")
    print(f"vault  {read_uint(rpc, bridge, SELECTOR_VAULT_BALANCE)}")


def claim_vault(rpc: str, bridge: str, preimage: int, broadcast: bool) -> None:
    require_galleon_chain()
    if read_bool(rpc, bridge, SELECTOR_CLAIMED):
        raise RuntimeError("bridge already claimed")
    payout = read_uint(rpc, bridge, SELECTOR_PAYOUT)
    vault = read_uint(rpc, bridge, SELECTOR_VAULT_BALANCE)
    if vault < payout:
        raise RuntimeError(f"vault underfunded: {vault} < payout {payout}")
    data = encode_claim_demo(preimage)
    if not broadcast:
        print(f"dry-run claimDemo({preimage}) on {bridge}")
        return
    key = galleon_key()
    tx = send_contract(key, bridge, data, CLAIM_GAS)
    print(f"claim  {tx}")
    print(f"       {GALLEON_EXPLORER}/tx/{tx}")


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Galleon HtlcBridgeRelease fund/claim")
    parser.add_argument("--bridge", default="", help="override GALLEON_HTLC_BRIDGE")
    parser.add_argument("--status", action="store_true")
    parser.add_argument("--fund", action="store_true")
    parser.add_argument("--claim", action="store_true")
    parser.add_argument("--amount", default="0.001", help="wiKAS to deposit when funding")
    parser.add_argument("--preimage", type=int, default=PAYMENT_PREIMAGE_HASH)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    args = parser.parse_args()

    bridge = (args.bridge or bridge_address()).strip()
    rpc = GALLEON_RPC
    wikas_decimals = token_meta(rpc, GALLEON_WRAPPED_IKAS)["decimals"]
    fund_wei = int(float(args.amount) * (10 ** wikas_decimals))

    if args.status or not (args.fund or args.claim):
        import json

        print(json.dumps(status(rpc, bridge), indent=2))
        return 0

    if args.fund:
        fund_vault(rpc, bridge, fund_wei, broadcast=args.broadcast and not args.dry_run)
    if args.claim:
        claim_vault(rpc, bridge, args.preimage, broadcast=args.broadcast and not args.dry_run)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
