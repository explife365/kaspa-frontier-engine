"""Minimal ERC-20 eth_call client. No web3 dependency.

Galleon USDC is Igra test liquidity, not Circle-issued cash.
"""

from __future__ import annotations

import json
import time
from typing import Any

from tn10_rest import decode_json_body, https_request, retry_after_wait

SELECTOR_DECIMALS = "0x313ce567"
SELECTOR_SYMBOL = "0x95d89b41"
SELECTOR_NAME = "0x06fdde03"
SELECTOR_BALANCE_OF = "70a08231"


def _strip0x(value: str) -> str:
    v = value.strip()
    if v.startswith(("0x", "0X")):
        return v[2:]
    return v


def balance_of_calldata(holder: str) -> str:
    hex_addr = _strip0x(holder)
    if len(hex_addr) != 40:
        raise ValueError("holder must be 20-byte hex")
    return f"0x{SELECTOR_BALANCE_OF}{'0' * 24}{hex_addr.lower()}"


def decode_uint256(hex_result: str) -> int:
    return int(_strip0x(hex_result) or "0", 16)


def decode_abi_string(hex_result: str) -> str:
    raw = bytes.fromhex(_strip0x(hex_result))
    if len(raw) < 64:
        raise ValueError("ABI string too short")
    offset = int.from_bytes(raw[24:32], "big")
    length = int.from_bytes(raw[offset + 24 : offset + 32], "big")
    start = offset + 32
    return raw[start : start + length].decode("utf-8")


def rpc_post(url: str, payload: object, timeout: float = 20.0) -> Any:
    if not url.startswith("https://"):
        raise RuntimeError(f"RPC endpoint must be https, got {url}")
    body = json.dumps(payload).encode()
    headers = {
        "Content-Type": "application/json",
        "Accept": "application/json",
        "Accept-Encoding": "identity",
        "User-Agent": "kaspa-frontier-engine/0.3",
        "Connection": "keep-alive",
    }
    last: BaseException | None = None
    for attempt in range(3):
        try:
            code, raw, encoding, retry_after = https_request(
                "POST", url, headers, timeout, body
            )
            if code in {408, 429, 500, 502, 503, 504} and attempt < 2:
                last = RuntimeError(f"RPC HTTP {code}")
                time.sleep(retry_after_wait(retry_after, attempt))
                continue
            if code >= 400:
                raise RuntimeError(f"RPC HTTP {code}")
            parsed = decode_json_body(raw, encoding)
            return parsed
        except RuntimeError as err:
            last = err
            if "timeout/connect" in str(err) and attempt < 2:
                time.sleep(retry_after_wait(None, attempt))
                continue
            raise
        except (TimeoutError, json.JSONDecodeError, OSError) as err:
            last = err
            if attempt >= 2:
                break
            time.sleep(retry_after_wait(None, attempt))
    raise RuntimeError(f"RPC timeout/connect for {url}") from last


def parse_rpc_batch_hex(parsed: object, n: int) -> list[str] | None:
    """Map JSON-RPC batch items by id 1..n. None if any slot is missing."""
    if n <= 0 or not isinstance(parsed, list):
        return None
    slots: list[str | None] = [None] * n
    for item in parsed:
        if not isinstance(item, dict) or item.get("error"):
            continue
        ident = item.get("id")
        result = item.get("result")
        if not isinstance(ident, int) or ident < 1 or ident > n:
            continue
        if not isinstance(result, str):
            continue
        slots[ident - 1] = result
    if any(slot is None for slot in slots):
        return None
    return [slot for slot in slots if slot is not None]


def rpc_call(url: str, method: str, params: list[Any], timeout: float = 20.0) -> Any:
    parsed = rpc_post(
        url,
        {"jsonrpc": "2.0", "id": 1, "method": method, "params": params},
        timeout,
    )
    if not isinstance(parsed, dict):
        raise RuntimeError("bad RPC result")
    if parsed.get("error"):
        raise RuntimeError(parsed["error"])
    return parsed.get("result")


def eth_call(url: str, to: str, data: str) -> str:
    result = rpc_call(url, "eth_call", [{"to": to, "data": data}, "latest"])
    if not isinstance(result, str):
        raise RuntimeError("eth_call returned no hex")
    return result


def token_meta(url: str, token: str) -> dict[str, Any]:
    batch = [
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [{"to": token, "data": SELECTOR_DECIMALS}, "latest"],
        },
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "eth_call",
            "params": [{"to": token, "data": SELECTOR_SYMBOL}, "latest"],
        },
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "eth_call",
            "params": [{"to": token, "data": SELECTOR_NAME}, "latest"],
        },
    ]
    hexes = parse_rpc_batch_hex(rpc_post(url, batch), 3)
    if hexes is None:
        hexes = [
            eth_call(url, token, SELECTOR_DECIMALS),
            eth_call(url, token, SELECTOR_SYMBOL),
            eth_call(url, token, SELECTOR_NAME),
        ]
    decimals_hex, symbol_hex, name_hex = hexes
    return {
        "address": token,
        "name": decode_abi_string(name_hex),
        "symbol": decode_abi_string(symbol_hex),
        "decimals": decode_uint256(decimals_hex),
        "circle_issued": False,
    }


def token_balance(url: str, token: str, holder: str) -> int:
    return decode_uint256(eth_call(url, token, balance_of_calldata(holder)))
