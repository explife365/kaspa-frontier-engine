"""Igra Galleon iKAS helpers. This crate does not mint iKAS.

Official faucet (EIP-191 signed challenge): https://faucet.igralabs.com
L1 Entry (lock tKAS, txid prefix 97b4): kaspatest:qqmstl2znv9tsfgcmj9shme82my867tapz7pdu4ztwdn6sm9452jj5mm0sxzw
"""

from __future__ import annotations

import hashlib
import math
from datetime import datetime, timezone
from typing import Any

ENTRY_PREFIX_BYTE = 0x92
GALLEON_TXID_PREFIX = "97b4"
GALLEON_ENTRY_ADDRESS = (
    "kaspatest:qqmstl2znv9tsfgcmj9shme82my867tapz7pdu4ztwdn6sm9452jj5mm0sxzw"
)
GALLEON_ENTRY_MIN_SOMPI = 100_000_000
IGRA_FAUCET = "https://faucet.igralabs.com"
GALLEON_RPC = "https://galleon-testnet.igralabs.com:8545"
GALLEON_CHAIN_ID = 38836
GALLEON_EXPLORER = "https://explorer.galleon-testnet.igralabs.com"
# Igra Galleon *test* USDC (6 decimals). Not Circle mainnet USDC. Not redeemable.
GALLEON_TEST_USDC = "0xFd89676CBb3D2742c565aFC02986370ef4ba667A"
# Crate-owned gTEST. Not USD. Not Circle.
GALLEON_GTEST = "0xbc5e27ab3ce2edb243593cda2437e5b30e0d5d7d"
# Wrapped iKAS (WETH9-style) live on Galleon. Not kaspad. Not USD.
GALLEON_WRAPPED_IKAS = "0x7331b0a33ac9aa92f506f057bfaa049ea133f77f"
# Canonical Circle USDC on Ethereum. Circle has not listed Galleon (38836).
CIRCLE_USDC_ETHEREUM = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
CIRCLE_USDC_ON_GALLEON = None
CIRCLE_NATIVE_USDC = {
    1: CIRCLE_USDC_ETHEREUM,
    8453: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
    42161: "0xaf88d065e77c8cC2239327C5EDb3A432268e5831",
    10: "0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85",
    137: "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359",
    43114: "0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E",
}


def same_addr(left: str, right: str) -> bool:
    return left.strip().lower() == right.strip().lower()


def circle_usdc_on_chain(chain_id: int) -> str | None:
    return CIRCLE_NATIVE_USDC.get(chain_id)


def is_circle_usdc(chain_id: int, token: str) -> bool:
    listed = circle_usdc_on_chain(chain_id)
    return listed is not None and same_addr(token, listed)


# Testnet floor is 2000 gwei. Igra's own faucet paid ~3000 gwei.
GALLEON_MIN_GAS_WEI = 2_000_000_000_000
GALLEON_RELAY_GAS_WEI = 3_000_000_000_000
NATIVE_TRANSFER_GAS = 21_000
# gTEST create simulation (Igra prepaid gas). Live; do not redeploy below this.
GTEST_CREATE_IKAS = 1.40
# wiKAS forge script simulate on Galleon: 753654 gas × 2000 gwei. Do not broadcast below this.
WIKAS_CREATE_IKAS = 1.507308
FAUCET_DRIP_IKAS = 0.1


def tx_total_wei(gas_limit: int, gas_price: int, value: int = 0) -> int:
    """Igra silently drops txs if balance < value + gasLimit * gasPrice."""
    return gas_limit * gas_price + value


def fits_balance(balance: int, gas_limit: int, gas_price: int, value: int = 0) -> bool:
    return balance >= tx_total_wei(gas_limit, gas_price, value)


def max_sendable_wei(
    balance: int,
    gas_limit: int = NATIVE_TRANSFER_GAS,
    gas_price: int = GALLEON_MIN_GAS_WEI,
) -> int:
    """Largest value that still passes Igra's prepaid gas check."""
    reserved = gas_limit * gas_price
    if balance <= reserved:
        return 0
    return balance - reserved


def parse_l2_address(addr: str) -> bytes:
    raw = addr.strip()
    if raw.startswith(("0x", "0X")):
        raw = raw[2:]
    if len(raw) != 40:
        raise ValueError("L2 address must be 20 bytes hex")
    try:
        out = bytes.fromhex(raw)
    except ValueError as err:
        raise ValueError("L2 address is not hex") from err
    if len(out) != 20:
        raise ValueError("L2 address must be 20 bytes")
    return out


def entry_payload(l2: bytes, amount_sompi: int, nonce_be: int) -> bytes:
    if len(l2) != 20:
        raise ValueError("L2 address must be 20 bytes")
    if amount_sompi < 0 or amount_sompi > 0xFFFFFFFFFFFFFFFF:
        raise ValueError("amount out of range")
    if nonce_be < 0 or nonce_be > 0xFFFFFFFF:
        raise ValueError("nonce out of range")
    return (
        bytes([ENTRY_PREFIX_BYTE])
        + l2
        + int(amount_sompi).to_bytes(8, "little")
        + int(nonce_be).to_bytes(4, "big")
    )


def txid_has_galleon_prefix(txid: str) -> bool:
    return txid.strip().lower().startswith(GALLEON_TXID_PREFIX)


def pow_leading_zero_bits(digest: bytes) -> int:
    bits = 0
    for byte in digest:
        if byte == 0:
            bits += 8
            continue
        bits += 8 - byte.bit_length()
        break
    return bits


def solve_pow(prefix: str, bits: int, limit: int = 2_000_000) -> int:
    """Find nonce where SHA-256(prefix + ':' + nonce) has `bits` leading zeros."""
    need = max(0, bits)
    for nonce in range(limit):
        digest = hashlib.sha256(f"{prefix}:{nonce}".encode()).digest()
        if pow_leading_zero_bits(digest) >= need:
            return nonce
    raise RuntimeError(f"PoW not found in {limit} tries")


def faucet_drip_body(
    address: str, signature: str, challenge: str, nonce: int | None = None
) -> dict[str, Any]:
    body: dict[str, Any] = {
        "address": address,
        "signature": signature,
        "challenge": challenge,
    }
    if nonce is not None:
        body["nonce"] = nonce
    return body


def utc_claim_day() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%d")


def parse_claim_line(line: str) -> tuple[str | None, str]:
    """Return (YYYY-MM-DD or None, address)."""
    raw = line.strip()
    if not raw or raw.startswith("#"):
        return None, ""
    parts = raw.split()
    if len(parts) >= 2 and len(parts[0]) == 10 and parts[0][4:5] == "-" and parts[0][7:8] == "-":
        return parts[0], parts[-1].lower()
    return None, raw.lower()


def stamp_undated_claim_lines(text: str, day: str | None = None) -> str:
    """Rewrite bare addresses as `YYYY-MM-DD 0x…` so they expire the next UTC day."""
    day = day or utc_claim_day()
    out: list[str] = []
    for line in text.splitlines():
        dated, addr = parse_claim_line(line)
        if not addr:
            continue
        out.append(f"{dated or day} {addr}")
    return "\n".join(out) + ("\n" if out else "")


def claim_recorded_today(line: str, addr: str, day: str | None = None) -> bool:
    day = day or utc_claim_day()
    dated, stored = parse_claim_line(line)
    if not stored or stored != addr.strip().lower():
        return False
    if dated is None:
        return False
    return dated == day


def faucet_blocks_connection(text: str) -> bool:
    lower = text.lower()
    return (
        "this connection" in lower
        or "for this ip" in lower
        or "for this connection" in lower
    )


def faucet_is_busy(text: str) -> bool:
    """True when Igra asks to retry (not an address/day or connection cap)."""
    lower = text.lower()
    if faucet_blocks_connection(text) or faucet_blocks_address(text):
        return False
    return (
        "try again in" in lower
        or "busy" in lower
        or "cooldown" in lower
        or "challenge not found" in lower
        or "expired" in lower
    )


def sweep_gas_ikas() -> float:
    """21k × 2000 gwei left on each extra after a full drip+sweep."""
    return NATIVE_TRANSFER_GAS * GALLEON_MIN_GAS_WEI / 1e18


def sweep_net_ikas() -> float:
    """iKAS that lands on primary after one 0.1 drip and a native sweep."""
    return FAUCET_DRIP_IKAS - sweep_gas_ikas()


def extras_to_reach(primary_ikas: float, target_ikas: float = GTEST_CREATE_IKAS) -> int:
    """Estimate test drips under the faucet's published fair-use limits."""
    short = target_ikas - primary_ikas
    if short <= 0:
        return 0
    net = sweep_net_ikas()
    if net <= 0:
        raise ValueError("sweep would not move iKAS")
    return math.ceil(short / net)


def faucet_blocks_address(text: str) -> bool:
    lower = text.lower()
    return (
        "already claimed" in lower
        or "0.1 ikas per day" in lower
        or ("daily limit" in lower and "connection" not in lower and "this ip" not in lower)
    )
