"""Claim Galleon iKAS from the official Igra faucet. Does not mint iKAS.

  python examples/galleon_faucet.py --status
  python examples/galleon_faucet.py --ensure-wallet
  python examples/galleon_faucet.py --drip

Keys stay in gitignored kaspa.env as GALLEON_PRIVATE_KEY. Never printed.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
import time
from contextlib import contextmanager
from pathlib import Path

from eth_account import Account
from eth_account.messages import encode_defunct

ROOT = Path(__file__).resolve().parents[1]
CLAIM_LOG = ROOT / ".local" / "galleon_faucet_claims.txt"
sys.path.insert(0, str(ROOT / "scripts"))
from galleon import (  # noqa: E402
    GALLEON_CHAIN_ID,
    GALLEON_ENTRY_ADDRESS,
    GALLEON_EXPLORER,
    GALLEON_MIN_GAS_WEI,
    GALLEON_RELAY_GAS_WEI,
    GALLEON_RPC,
    GALLEON_TXID_PREFIX,
    GALLEON_WRAPPED_IKAS,
    WIKAS_CREATE_IKAS,
    IGRA_FAUCET,
    NATIVE_TRANSFER_GAS,
    extras_to_reach,
    claim_recorded_today,
    faucet_blocks_address,
    faucet_blocks_connection,
    faucet_drip_body,
    faucet_is_busy,
    fits_balance,
    max_sendable_wei,
    solve_pow,
    stamp_undated_claim_lines,
    utc_claim_day,
)
from kaspa_env import load_kaspa_env, upsert_kaspa_env  # noqa: E402
from tn10_rest import decode_json_body, https_request, retry_after_wait  # noqa: E402

USER_AGENT = "kaspa-frontier-engine/0.3"
load_kaspa_env(ROOT)


def http_json(method: str, url: str, body: dict | None = None, timeout: float = 45.0) -> dict:
    data = None if body is None else json.dumps(body).encode()
    headers = {
        "User-Agent": USER_AGENT,
        "Accept": "application/json",
        "Accept-Encoding": "identity",
        "Content-Type": "application/json",
        "Connection": "keep-alive",
    }
    code, raw, encoding, _retry_after = https_request(method, url, headers, timeout, data)
    parsed: object = {}
    text = ""
    if raw:
        try:
            parsed = decode_json_body(raw, encoding)
        except (json.JSONDecodeError, UnicodeDecodeError, OSError):
            text = raw.decode("utf-8", "replace")[:500]
            parsed = {}
    if code >= 400:
        if isinstance(parsed, dict):
            msg = parsed.get("error") or parsed.get("message") or text
        else:
            msg = text
        if not msg:
            msg = raw.decode("utf-8", "replace")[:500] if raw else f"HTTP {code}"
        raise RuntimeError(f"HTTP {code}: {msg}")
    if not isinstance(parsed, dict):
        raise RuntimeError("faucet returned non-object JSON")
    return parsed


def rpc_hex(method: str, params: list) -> str:
    payload = json.dumps(
        {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
    ).encode()
    headers = {
        "User-Agent": USER_AGENT,
        "Accept": "application/json",
        "Accept-Encoding": "identity",
        "Content-Type": "application/json",
        "Connection": "keep-alive",
    }
    last_err: Exception | None = None
    body: dict | None = None
    for attempt in range(3):
        try:
            code, raw, encoding, retry_after = https_request(
                "POST", GALLEON_RPC, headers, 45, payload
            )
            if code in {408, 429, 500, 502, 503, 504} and attempt < 2:
                last_err = RuntimeError(f"Galleon RPC HTTP {code}")
                time.sleep(retry_after_wait(retry_after, attempt))
                continue
            if code >= 400:
                raise RuntimeError(f"Galleon RPC HTTP {code}")
            parsed = decode_json_body(raw, encoding)
            if not isinstance(parsed, dict):
                raise RuntimeError("bad RPC result")
            body = parsed
            break
        except RuntimeError as err:
            last_err = err
            if "timeout/connect" in str(err) and attempt < 2:
                time.sleep(2 * (attempt + 1))
                continue
            raise
        except (TimeoutError, json.JSONDecodeError) as err:
            last_err = err
            if attempt >= 2:
                break
            time.sleep(2 * (attempt + 1))
    else:
        raise RuntimeError(f"Galleon RPC timeout: {last_err}") from last_err
    if body is None:
        raise RuntimeError(f"Galleon RPC timeout: {last_err}") from last_err
    if "error" in body:
        raise RuntimeError(str(body["error"]))
    result = body.get("result")
    if not isinstance(result, str):
        raise RuntimeError("bad RPC result")
    return result


def rpc_hex_batch(calls: list[tuple[str, list]]) -> list[str]:
    """One JSON-RPC batch POST. Falls back to sequential rpc_hex."""
    if not calls:
        return []
    payload = json.dumps(
        [
            {"jsonrpc": "2.0", "id": i + 1, "method": method, "params": params}
            for i, (method, params) in enumerate(calls)
        ]
    ).encode()
    headers = {
        "User-Agent": USER_AGENT,
        "Accept": "application/json",
        "Accept-Encoding": "identity",
        "Content-Type": "application/json",
        "Connection": "keep-alive",
    }
    try:
        code, raw, encoding, retry_after = https_request(
            "POST", GALLEON_RPC, headers, 45, payload
        )
        if code in {408, 429, 500, 502, 503, 504}:
            time.sleep(retry_after_wait(retry_after, 0))
            raise RuntimeError(f"Galleon RPC HTTP {code}")
        if code >= 400:
            raise RuntimeError(f"Galleon RPC HTTP {code}")
        parsed = decode_json_body(raw, encoding)
        if not isinstance(parsed, list):
            raise RuntimeError("bad RPC batch result")
        slots: list[str | None] = [None] * len(calls)
        for item in parsed:
            if not isinstance(item, dict) or item.get("error"):
                continue
            ident = item.get("id")
            result = item.get("result")
            if not isinstance(ident, int) or ident < 1 or ident > len(calls):
                continue
            if isinstance(result, str):
                slots[ident - 1] = result
        if all(slot is not None for slot in slots):
            return [slot for slot in slots if slot is not None]
    except (RuntimeError, TimeoutError, json.JSONDecodeError, OSError):
        pass
    return [rpc_hex(method, params) for method, params in calls]


def wei_to_ikas(wei_hex: str) -> float:
    return int(wei_hex, 16) / 1e18


def print_status() -> None:
    health = http_json("GET", f"{IGRA_FAUCET}/api/health")
    status = http_json("GET", f"{IGRA_FAUCET}/api/testnet/status")
    print(f"faucet  {IGRA_FAUCET}  health={health.get('ok')}  version={health.get('version')}")
    print(f"network {status.get('network')}  chainId {status.get('chainId')}")
    print(f"dispenser {status.get('faucetAddress')}  {status.get('balance')}")
    limits = status.get("limits") or {}
    print(
        f"limits  perRequest={limits.get('perRequest')}  "
        f"daily={limits.get('dailyPerAddress')}  maxWallet={limits.get('maxUserBalance')}"
    )
    print("this crate does not mint iKAS; it claims from Igra or locks tKAS via Entry")
    print(f"L1 entry {GALLEON_ENTRY_ADDRESS}  txid prefix {GALLEON_TXID_PREFIX}")
    print("test iKAS guidance:")
    print("  1. Use one wallet and the faucet's published fair-use limits.")
    print("  2. If you already have TN10 tKAS: official grind UI only")
    print("     (Kasperia / ikas.katbridge.com). txid must start with 97b4.")
    print("     This crate encodes Entry payload; it does not grind.")
    print("  3. GalleonIkasFaucet is unfunded. It cannot mint or speed the official faucet.")
    print("  4. Do not hammer the faucet, share one CGNAT IP, or send tKAS without 97b4.")
    try:
        primary = address_of(galleon_key())
        bal = wei_to_ikas(rpc_hex("eth_getBalance", [primary, "latest"]))
        if GALLEON_WRAPPED_IKAS:
            print(
                f"primary {bal:.6f} iKAS; wiKAS live {GALLEON_WRAPPED_IKAS} "
                "(WETH9-style wrap; not kaspad; not USD)"
            )
        else:
            need = extras_to_reach(bal, WIKAS_CREATE_IKAS)
            print(
                f"primary {bal:.6f} iKAS; wiKAS prepaid ~{WIKAS_CREATE_IKAS:.3f}; "
                f"~{need} more test drips under the faucet's published fair-use limits"
            )
    except Exception as err:
        print(f"primary plan skipped ({err})")


def galleon_key() -> str:
    key = (os.environ.get("GALLEON_PRIVATE_KEY") or "").strip()
    if not key:
        raise RuntimeError("missing GALLEON_PRIVATE_KEY; run --ensure-wallet")
    if not key.startswith("0x"):
        key = "0x" + key
    return key


def address_of(key: str) -> str:
    addr = Account.from_key(key).address
    if not addr.startswith("0x") or len(addr) != 42:
        raise RuntimeError("cast wallet address failed")
    return addr


def ensure_wallet() -> str:
    existing = (os.environ.get("GALLEON_PRIVATE_KEY") or "").strip()
    if existing:
        if not existing.startswith("0x"):
            existing = "0x" + existing
        addr = address_of(existing)
        upsert_kaspa_env({"GALLEON_ADDRESS": addr}, ROOT)
        os.environ["GALLEON_ADDRESS"] = addr
        print(f"wallet  {addr}")
        print(f"        {GALLEON_EXPLORER}/address/{addr}")
        return addr
    print("creating GALLEON_PRIVATE_KEY in kaspa.env (not printed)")
    key = "0x" + os.urandom(32).hex()
    addr = address_of(key)
    upsert_kaspa_env({"GALLEON_PRIVATE_KEY": key, "GALLEON_ADDRESS": addr}, ROOT)
    os.environ["GALLEON_PRIVATE_KEY"] = key
    os.environ["GALLEON_ADDRESS"] = addr
    print(f"wallet  {addr}")
    print(f"        {GALLEON_EXPLORER}/address/{addr}")
    return addr


def sign_challenge(key: str, challenge: str) -> str:
    sig = Account.sign_message(encode_defunct(text=challenge), key).signature.hex()
    if not sig.startswith("0x"):
        sig = "0x" + sig
    if not sig.startswith("0x") or len(sig) < 130:
        raise RuntimeError("cast wallet sign failed")
    return sig


def print_balance(addr: str) -> float:
    wei, chain_hex = rpc_hex_batch(
        [
            ("eth_getBalance", [addr, "latest"]),
            ("eth_chainId", []),
        ]
    )
    ikas = wei_to_ikas(wei)
    print(f"balance {ikas} iKAS  ({int(wei, 16)} wei)")
    print(f"chain   {int(chain_hex, 16)} (want {GALLEON_CHAIN_ID})")
    return ikas


def require_galleon_chain() -> None:
    chain_id = int(rpc_hex("eth_chainId", []), 16)
    if chain_id != GALLEON_CHAIN_ID:
        raise RuntimeError(
            f"refusing value transfer on chain {chain_id}; expected Galleon {GALLEON_CHAIN_ID}"
        )


def require_l2_address(value: str) -> str:
    address = value.strip()
    if (
        len(address) != 42
        or not address.startswith("0x")
        or not all(char in "0123456789abcdefABCDEF" for char in address[2:])
    ):
        raise ValueError("need a 20-byte 0x L2 address")
    return address


def extra_key_env(index: int) -> str:
    return f"GALLEON_PRIVATE_KEY_{index}"


def extra_addr_env(index: int) -> str:
    return f"GALLEON_ADDRESS_{index}"


def extra_indices() -> list[int]:
    found: list[int] = []
    prefix = "GALLEON_PRIVATE_KEY_"
    for key, val in os.environ.items():
        if not key.startswith(prefix) or not (val or "").strip():
            continue
        suffix = key[len(prefix) :]
        if suffix.isdigit():
            index = int(suffix)
            if 2 <= index <= 1024:
                found.append(index)
    found.sort()
    return found


def load_key(env_name: str) -> str:
    key = (os.environ.get(env_name) or "").strip()
    if not key:
        raise RuntimeError(f"missing {env_name}")
    if not key.startswith("0x"):
        key = "0x" + key
    return key


def ensure_extra_range(
    first: int, last: int, *, list_existing: bool = False
) -> list[tuple[int, str]]:
    """Create GALLEON_PRIVATE_KEY_{first}..{last}. Skip rewriting existing keys."""
    if first < 2:
        first = 2
    if last < first:
        raise ValueError("last extra must be >= first")
    created: list[tuple[int, str]] = []
    existing_n = 0
    for index in range(first, last + 1):
        raw = (os.environ.get(extra_key_env(index)) or "").strip()
        if raw:
            key = raw if raw.startswith("0x") else "0x" + raw
            addr = address_of(key)
            stored = (os.environ.get(extra_addr_env(index)) or "").strip()
            if stored.lower() != addr.lower():
                upsert_kaspa_env({extra_addr_env(index): addr}, ROOT)
                os.environ[extra_addr_env(index)] = addr
            if list_existing:
                print(f"wallet{index}  {addr}  (existing)")
            existing_n += 1
        else:
            key = "0x" + os.urandom(32).hex()
            addr = address_of(key)
            upsert_kaspa_env(
                {extra_key_env(index): key, extra_addr_env(index): addr}, ROOT
            )
            os.environ[extra_key_env(index)] = key
            os.environ[extra_addr_env(index)] = addr
            print(f"wallet{index}  {addr}  (created)")
        created.append((index, addr))
    if existing_n and not list_existing:
        print(f"wallets {first}..{last}: {existing_n} existing, {len(created) - existing_n} created")
    return created


def ensure_extra_wallets(count: int) -> list[tuple[int, str]]:
    """Create GALLEON_PRIVATE_KEY_2..N in kaspa.env. Never prints keys."""
    if count < 1:
        raise ValueError("need at least 1 extra account")
    return ensure_extra_range(2, 1 + count, list_existing=True)


def sign_faucet_challenge(key: str, addr: str) -> tuple[str, str, int | None]:
    challenge_body = http_json(
        "POST", f"{IGRA_FAUCET}/api/testnet/challenge", {"address": addr}
    )
    challenge = challenge_body.get("challenge")
    if not isinstance(challenge, str) or not challenge:
        raise RuntimeError("no challenge in faucet response")
    nonce = None
    bits = challenge_body.get("difficulty") or challenge_body.get("bits")
    prefix = challenge_body.get("powPrefix") or challenge_body.get("prefix")
    if isinstance(bits, int) and bits > 0:
        pow_prefix = prefix if isinstance(prefix, str) and prefix else challenge
        print(f"solving PoW bits={bits}")
        nonce = solve_pow(pow_prefix, bits)
    print("signing EIP-191 challenge")
    return challenge, sign_challenge(key, challenge), nonce


def busy_wait_seconds(text: str, attempt: int) -> int:
    marker = "try again in "
    lower = text.lower()
    if marker in lower:
        after = lower.split(marker, 1)[1]
        digits = ""
        for ch in after:
            if ch.isdigit():
                digits += ch
            elif digits:
                break
        if digits:
            return min(int(digits) + 2, 60)
    return min(12 * (attempt + 1), 40)


def claimed_today(addr: str) -> bool:
    if not CLAIM_LOG.is_file():
        return False
    day = utc_claim_day()
    raw = CLAIM_LOG.read_text(encoding="utf-8-sig")
    stamped = stamp_undated_claim_lines(raw, day)
    return any(claim_recorded_today(line, addr, day) for line in stamped.splitlines())


@contextmanager
def exclusive_file_lock(lock_path: Path):
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with lock_path.open("a+b") as handle:
        if os.name == "nt":
            import msvcrt

            handle.seek(0, os.SEEK_END)
            if handle.tell() == 0:
                handle.write(b"\0")
                handle.flush()
            handle.seek(0)
            msvcrt.locking(handle.fileno(), msvcrt.LK_LOCK, 1)
            try:
                yield
            finally:
                handle.seek(0)
                msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl

            fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
            try:
                yield
            finally:
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


@contextmanager
def claim_log_lock():
    lock_path = CLAIM_LOG.with_suffix(CLAIM_LOG.suffix + ".lock")
    with exclusive_file_lock(lock_path):
        yield


@contextmanager
def claim_address_lock(addr: str):
    normalized = addr.strip().lower().replace("0x", "", 1)
    lock_path = CLAIM_LOG.with_name(f"{CLAIM_LOG.name}.{normalized}.claim.lock")
    with exclusive_file_lock(lock_path):
        yield


def remember_claim(addr: str) -> None:
    normalized = addr.strip().lower()
    with claim_log_lock():
        raw = CLAIM_LOG.read_text(encoding="utf-8-sig") if CLAIM_LOG.is_file() else ""
        day = utc_claim_day()
        stamped = stamp_undated_claim_lines(raw, day)
        if any(claim_recorded_today(line, normalized, day) for line in stamped.splitlines()):
            return
        body = stamped
        if body and not body.endswith("\n"):
            body += "\n"
        body += f"{day} {normalized}\n"
        with tempfile.NamedTemporaryFile(
            "w", encoding="utf-8", dir=CLAIM_LOG.parent, delete=False
        ) as handle:
            handle.write(body)
            handle.flush()
            os.fsync(handle.fileno())
            temp_path = Path(handle.name)
        os.replace(temp_path, CLAIM_LOG)


def drip_with_key(key: str, label: str, *, connection_spent: bool = False) -> bool:
    """Claim 0.1 iKAS. Returns True if this call received a drip."""
    addr = address_of(key)
    print(f"{label}  {addr}")
    with claim_address_lock(addr):
        return _drip_with_key_locked(key, addr, connection_spent=connection_spent)


def _drip_with_key_locked(key: str, addr: str, *, connection_spent: bool) -> bool:
    if claimed_today(addr):
        print("skipped: already claimed on this faucet day (no request)")
        return False
    bal = print_balance(addr)
    if bal >= 0.1:
        print("already at/above faucet maxUserBalance (0.1 iKAS); not claiming")
        remember_claim(addr)
        return False
    last_err: str | None = None
    for attempt in range(5):
        challenge, signature, nonce = sign_faucet_challenge(key, addr)
        try:
            result = http_json(
                "POST",
                f"{IGRA_FAUCET}/api/testnet/drip",
                faucet_drip_body(addr, signature, challenge, nonce),
            )
            print(json.dumps({k: result.get(k) for k in result if k != "privateKey"}, indent=2))
            tx = result.get("txHash") or result.get("hash") or result.get("transactionHash")
            if isinstance(tx, str):
                print(f"explorer {GALLEON_EXPLORER}/tx/{tx}")
            remember_claim(addr)
            time.sleep(2)
            print_balance(addr)
            return True
        except RuntimeError as err:
            last_err = str(err)
            if faucet_blocks_connection(last_err):
                raise
            if faucet_blocks_address(last_err):
                print("skipped: this address already claimed today")
                remember_claim(addr)
                return False
            retryable = faucet_is_busy(last_err)
            if retryable and connection_spent:
                raise RuntimeError(
                    "HTTP 429: Daily limit reached for this connection. Try again tomorrow."
                ) from err
            if retryable:
                wait_s = busy_wait_seconds(last_err, attempt)
                print(f"faucet retry ({last_err}); waiting {wait_s}s then new challenge")
                time.sleep(wait_s)
                continue
            raise
    raise RuntimeError(last_err or "drip failed")


def _skip_drip_error(label: str, key: str, err: RuntimeError) -> bool:
    """Return True if the drip loop should stop (connection cap / faucet busy)."""
    text = str(err)
    if faucet_blocks_connection(text):
        print(f"{label}  skipped: faucet daily connection cap ({text})")
        print("remaining extras keep their keys for tomorrow")
        return True
    if faucet_blocks_address(text):
        print(f"{label}  skipped: this address already claimed today")
        remember_claim(address_of(key))
        time.sleep(3)
        return False
    if faucet_is_busy(text) or "429" in text:
        print(f"{label}  skipped: faucet busy ({text})")
        print("remaining extras keep their keys; respect the faucet limit and retry later")
        return True
    raise err


def drip_all(start: int = 2, include_primary: bool = True) -> None:
    # Keep extra keys ahead of --from so a high index is not a silent no-op.
    last_needed = start + 8
    ensure_extra_range(start, last_needed, list_existing=False)
    claimed_this_run = False
    if include_primary:
        primary = galleon_key()
        try:
            if drip_with_key(primary, "wallet", connection_spent=claimed_this_run):
                claimed_this_run = True
        except RuntimeError as err:
            if _skip_drip_error("wallet", primary, err):
                return
    for index in extra_indices():
        if index < start:
            continue
        key = load_key(extra_key_env(index))
        try:
            claimed = drip_with_key(
                key, f"wallet{index}", connection_spent=claimed_this_run
            )
        except RuntimeError as err:
            if _skip_drip_error(f"wallet{index}", key, err):
                print(f"claim loop stopped at wallet{index}")
                return
            continue
        if claimed:
            claimed_this_run = True
            time.sleep(8)


def send_from_key(key: str, to: str, wei: int, gas_price: int) -> None:
    to = require_l2_address(to)
    if wei <= 0:
        raise ValueError("transfer amount must be > 0 wei")
    if gas_price <= 0:
        raise ValueError("gas price must be > 0")
    require_galleon_chain()
    src = address_of(key)
    bal = int(rpc_hex("eth_getBalance", [src, "latest"]), 16)
    if not fits_balance(bal, NATIVE_TRANSFER_GAS, gas_price, wei):
        need = NATIVE_TRANSFER_GAS * gas_price + wei
        raise RuntimeError(
            f"would be silently dropped on Igra: have {bal} wei, need {need} "
            f"(value + gasLimit*{gas_price})"
        )
    print(
        f"push  {wei / 1e18} iKAS  {src} -> {to}  "
        f"gas {NATIVE_TRANSFER_GAS} @ {gas_price} wei"
    )
    nonce = int(rpc_hex("eth_getTransactionCount", [src, "pending"]), 16)
    signed = Account.sign_transaction(
        {
            "chainId": GALLEON_CHAIN_ID,
            "nonce": nonce,
            "to": to,
            "value": wei,
            "gas": NATIVE_TRANSFER_GAS,
            "gasPrice": gas_price,
        },
        key,
    )
    raw = signed.raw_transaction.hex()
    if not raw.startswith("0x"):
        raw = "0x" + raw
    tx_hash = rpc_hex("eth_sendRawTransaction", [raw])
    print(f"tx    {tx_hash}")
    print(f"      {GALLEON_EXPLORER}/tx/{tx_hash}")
    print_balance(src)


def sweep_extras_to_primary(start: int = 2) -> None:
    require_galleon_chain()
    dest = address_of(galleon_key())
    print(f"sweep -> {dest}  from {start}")
    for index in extra_indices():
        if index < start:
            continue
        key = load_key(extra_key_env(index))
        src = address_of(key)
        bal = int(rpc_hex("eth_getBalance", [src, "latest"]), 16)
        wei = max_sendable_wei(bal, NATIVE_TRANSFER_GAS, GALLEON_MIN_GAS_WEI)
        if wei <= 0:
            print(f"wallet{index}  {src}  skip (no sendable iKAS)")
            continue
        send_from_key(key, dest, wei, GALLEON_MIN_GAS_WEI)
    print("primary after sweep")
    print_balance(dest)


def drip() -> None:
    drip_with_key(galleon_key(), "wallet")


def send_native(to: str, ikas: float) -> None:
    """Owner-push native iKAS. 21_000 gas — same as Igra's official faucet tx."""
    if ikas <= 0:
        raise ValueError("amount must be > 0")
    to = require_l2_address(to)
    key = galleon_key()
    require_galleon_chain()
    wei = int(ikas * 1e18)
    send_from_key(key, to, wei, GALLEON_RELAY_GAS_WEI)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Claim Galleon iKAS from Igra's faucet")
    parser.add_argument("--status", action="store_true")
    parser.add_argument("--ensure-wallet", action="store_true")
    parser.add_argument("--drip", action="store_true", help="claim 0.1 iKAS (official Igra faucet)")
    parser.add_argument("--balance", action="store_true")
    parser.add_argument("--send", metavar="0xADDR", help="owner-push native iKAS (21k gas)")
    parser.add_argument("--ikas", type=float, help="amount for --send")
    return parser.parse_args()


def main() -> None:
    if hasattr(sys.stdout, "reconfigure"):
        try:
            sys.stdout.reconfigure(line_buffering=True)
        except Exception:
            pass
    args = parse_args()
    if args.status:
        print_status()
        return
    if args.ensure_wallet:
        ensure_wallet()
        return
    if args.balance:
        print_balance(address_of(galleon_key()))
        return
    if args.drip:
        ensure_wallet()
        drip()
        return
    if args.send:
        if args.ikas is None:
            raise SystemExit("--send needs --ikas")
        send_native(args.send, args.ikas)
        return
    raise SystemExit(
        "usage: galleon_faucet.py --status | --ensure-wallet | "
        "--balance | --drip | --send 0x.. --ikas 0.01"
    )


if __name__ == "__main__":
    main()
