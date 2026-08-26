"""TN10 explorer REST with retries and a User-Agent (bare urllib gets 403).

Nodes often mishandle Content-Encoding:gzip (truncated bodies). Request identity
only; still accept gzip if a proxy sends it, with a raw-JSON fallback.
"""

from __future__ import annotations

import gzip
import http.client
import json
import ssl
import time
import urllib.parse
from typing import Any

REST_BASE = "https://api-tn10.kaspa.org"
USER_AGENT = "kaspa-frontier-engine/0.3"
GET_ATTEMPTS = 3
COINBASE_MATURITY_DAA = 1000
_TLS = ssl.create_default_context()
_HTTPS: dict[str, http.client.HTTPSConnection] = {}
# KIP-0009. C=1e12: a 0.1 tKAS output from a ~0.5 tKAS input exceeds max storage mass.
STORAGE_MASS_PARAMETER = 10**12
MAX_STORAGE_MASS = 100_000
MIN_STORAGE_SAFE_SOMPI = 20_000_000
# wasm mass fee can land under Toccata's 100 sompi/gram floor.
MIN_PRIORITY_FEE_SOMPI = 50_000
# wasm create_transactions subtracts priority fee from change; that can
# push a 0.2 tKAS change under the KIP-0009 floor and fail after our check.
FEE_PAD_SOMPI = 1_000_000


def encode_path_segment(value: str) -> str:
    """Keep unreserved chars; percent-encode `:` in `kaspatest:` / `kaspa:`."""
    return urllib.parse.quote(value, safe="-._~")


def decode_json_body(raw: bytes, content_encoding: str = "") -> Any:
    """Parse JSON. Nodes sometimes gzip without a matching header."""
    encoding = (content_encoding or "").lower()
    body = raw
    if encoding == "gzip":
        try:
            body = gzip.decompress(raw)
        except OSError:
            body = raw
    try:
        return json.loads(body.decode())
    except (json.JSONDecodeError, UnicodeDecodeError):
        try:
            return json.loads(gzip.decompress(body).decode())
        except Exception:
            raise


def https_request(
    method: str,
    url: str,
    headers: dict[str, str],
    timeout_s: float,
    body: bytes | None = None,
) -> tuple[int, bytes, str, str]:
    """HTTPS GET/POST with TLS keep-alive. Reconnect once on a dead pooled socket.

    Returns status, body, Content-Encoding, Retry-After (empty if absent).
    """
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme != "https" or not parsed.netloc:
        raise RuntimeError(f"REST endpoint must be https, got {url}")
    path = parsed.path or "/"
    if parsed.query:
        path = f"{path}?{parsed.query}"
    host = parsed.netloc
    verb = method.upper()
    last: BaseException | None = None
    for reuse in (True, False):
        try:
            if not reuse:
                old = _HTTPS.pop(host, None)
                if old is not None:
                    try:
                        old.close()
                    except OSError:
                        pass
            conn = _HTTPS.get(host)
            if conn is None:
                conn = http.client.HTTPSConnection(host, timeout=timeout_s, context=_TLS)
                _HTTPS[host] = conn
            else:
                conn.timeout = timeout_s
            conn.request(verb, path, body=body, headers=dict(headers))
            resp = conn.getresponse()
            raw = resp.read()
            encoding = (resp.getheader("Content-Encoding") or "").lower()
            retry_after = resp.getheader("Retry-After") or ""
            return resp.status, raw, encoding, retry_after
        except (TimeoutError, OSError, http.client.HTTPException, ssl.SSLError) as err:
            last = err
            dead = _HTTPS.pop(host, None)
            if dead is not None:
                try:
                    dead.close()
                except OSError:
                    pass
            if not reuse:
                break
    raise RuntimeError(f"REST timeout/connect for {url}") from last


def https_get(url: str, headers: dict[str, str], timeout_s: float) -> tuple[int, bytes, str, str]:
    """GET with TLS keep-alive. Reconnect once on a dead pooled socket."""
    return https_request("GET", url, headers, timeout_s)


def retry_after_wait(retry_after: str | None, attempt: int) -> float:
    """Honor Retry-After on 429/503, capped at 2s so CEX snapshots cannot stall."""
    backoff = 0.15 * (2**attempt)
    if retry_after:
        try:
            secs = int(retry_after.strip())
            return max(backoff, min(float(secs), 2.0))
        except ValueError:
            pass
    return backoff


def storage_mass(input_amounts: list[int], output_amounts: list[int]) -> int:
    """Net KIP-0009 storage mass. Harmonic terms use integer division like kaspad."""

    def terms(amounts: list[int]) -> int:
        total = 0
        for amount in amounts:
            if amount > 0:
                total += STORAGE_MASS_PARAMETER // amount
        return total

    return max(0, terms(output_amounts) - terms(input_amounts))


def utxo_amount(entry: dict[str, Any]) -> int:
    raw = utxo_entry(entry)
    return int(raw.get("amount") or 0)


def select_commit_entries(mature: list[dict[str, Any]], requested: int) -> tuple[list[dict[str, Any]], int]:
    """Spend enough UTXOs that P2SH and change stay storage-mass safe after fees."""
    if not mature:
        raise RuntimeError("no spendable UTXOs")
    ordered = sorted(mature, key=utxo_amount, reverse=True)
    change_floor = MIN_STORAGE_SAFE_SOMPI + FEE_PAD_SOMPI
    commit_floor = MIN_STORAGE_SAFE_SOMPI
    need = max(requested, commit_floor) + change_floor
    selected: list[dict[str, Any]] = []
    total = 0
    for entry in ordered:
        selected.append(entry)
        total += utxo_amount(entry)
        if total >= need:
            break
    commit = max(requested, commit_floor)
    change = total - commit
    if change < change_floor:
        commit = total - change_floor
        change = change_floor
    if commit < commit_floor:
        commit = total // 2
        change = total - commit
    inputs = [utxo_amount(entry) for entry in selected]
    mass = storage_mass(inputs, [commit, change])
    if (
        commit < commit_floor
        or change < MIN_STORAGE_SAFE_SOMPI
        or mass > MAX_STORAGE_MASS
    ):
        raise RuntimeError(
            f"need ~{need} sompi so commit and change stay "
            f">= {MIN_STORAGE_SAFE_SOMPI} after fees (storage mass {mass}). Wallet has {total}."
        )
    return selected, commit


def assert_storage_mass_safe(input_amounts: list[int], output_amounts: list[int]) -> None:
    mass = storage_mass(input_amounts, output_amounts)
    if mass > MAX_STORAGE_MASS:
        raise RuntimeError(
            f"KIP-0009 storage mass {mass} exceeds {MAX_STORAGE_MASS}. "
            f"Keep each output >= {MIN_STORAGE_SAFE_SOMPI} sompi (~0.2 tKAS) "
            "or spend almost the full wallet so there is no tiny change."
        )


def get_json(path: str, timeout_s: float = 12.0) -> Any:
    if not REST_BASE.startswith("https://"):
        raise RuntimeError(f"REST endpoint must be https, got {REST_BASE}")
    url = f"{REST_BASE}{path}"
    headers = {
        "User-Agent": USER_AGENT,
        "Accept": "application/json",
        "Accept-Encoding": "identity",
        "Connection": "keep-alive",
    }
    last: BaseException | None = None
    for attempt in range(GET_ATTEMPTS):
        try:
            code, raw, encoding, retry_after = https_get(url, headers, timeout_s)
            if code in {408, 429, 500, 502, 503, 504} and attempt + 1 < GET_ATTEMPTS:
                last = RuntimeError(f"REST {code} for {url}")
                time.sleep(retry_after_wait(retry_after, attempt))
                continue
            if code >= 400:
                raise RuntimeError(f"REST {code} for {url}")
            return decode_json_body(raw, encoding)
        except RuntimeError as err:
            text = str(err)
            if "timeout/connect" in text and attempt + 1 < GET_ATTEMPTS:
                last = err
                time.sleep(0.15 * (2**attempt))
                continue
            raise
        except (TimeoutError, OSError, json.JSONDecodeError) as err:
            last = err
            if attempt + 1 >= GET_ATTEMPTS:
                raise RuntimeError(f"REST timeout/connect for {url}") from err
            time.sleep(0.15 * (2**attempt))
    raise RuntimeError(f"REST failed for {url}: {last}")


def _address_json(paths: list[str]) -> Any:
    for index, path in enumerate(paths):
        try:
            return get_json(path)
        except RuntimeError as error:
            fallback = index == 0 and any(
                marker in str(error) for marker in ("REST 400 ", "REST 403 ", "REST 404 ")
            )
            if not fallback:
                raise
    raise RuntimeError("all address REST path variants failed")


def address_balance_sompi(address: str) -> int:
    paths = [
        f"/addresses/{encode_path_segment(address)}/balance",
        f"/addresses/{address}/balance",
    ]
    body = _address_json(paths)
    if not isinstance(body, dict):
        raise RuntimeError("REST address balance response is not an object")
    raw = body.get("balance")
    try:
        return int(raw)
    except (TypeError, ValueError) as error:
        raise RuntimeError("REST address balance is missing or invalid") from error


def virtual_daa() -> int | None:
    try:
        body = get_json("/info/blockdag")
        return int(body["virtualDaaScore"])
    except (RuntimeError, KeyError, TypeError, ValueError):
        return None


def fee_estimate() -> dict[str, Any] | None:
    try:
        body = get_json("/info/fee-estimate")
    except RuntimeError:
        return None
    return body if isinstance(body, dict) else None


def utxo_entry(entry: dict[str, Any]) -> dict[str, Any]:
    raw = entry.get("utxoEntry") or entry.get("utxo_entry") or {}
    return raw if isinstance(raw, dict) else {}


def is_mature_utxo(entry: dict[str, Any], virtual_daa_score: int | None) -> bool:
    """Skip immature coinbase (1000 DAA). Non-coinbase is always spendable."""
    utxo = utxo_entry(entry)
    coinbase = bool(utxo.get("isCoinbase") or utxo.get("is_coinbase"))
    if not coinbase:
        return True
    if virtual_daa_score is None:
        return False
    try:
        block_daa = int(utxo.get("blockDaaScore") or utxo.get("block_daa_score") or 0)
    except (TypeError, ValueError):
        return False
    return virtual_daa_score >= block_daa + COINBASE_MATURITY_DAA


def spendable_entries(
    entries: list[dict[str, Any]], virtual_daa_score: int | None
) -> list[dict[str, Any]]:
    return [e for e in entries if is_mature_utxo(e, virtual_daa_score)]


DEFAULT_CONFIRMATIONS = 60


def address_utxos(address: str) -> list[dict[str, Any]]:
    paths = [
        f"/addresses/{encode_path_segment(address)}/utxos",
        f"/addresses/{address}/utxos",
    ]
    body = _address_json(paths)
    if not isinstance(body, list) or any(not isinstance(entry, dict) for entry in body):
        raise RuntimeError("REST address UTXO response is not an array of objects")
    return body


def daa_confirmations(virtual_daa_score: int, block_daa_score: int) -> int:
    return max(0, virtual_daa_score - block_daa_score)


def confirm_withdrawal(
    txid: str,
    destination: str,
    output_index: int,
    amount_sompi: int,
    utxos: list[dict[str, Any]],
    virtual_daa_score: int,
    required: int,
) -> dict[str, Any] | None:
    """None until dest UTXO exists and virtual_daa - block_daa >= required."""
    need = max(1, required)
    for utxo in utxos:
        out = utxo.get("outpoint") or {}
        if (
            not isinstance(out, dict)
            or out.get("transactionId") != txid
            or utxo.get("address") != destination
        ):
            continue
        entry = utxo_entry(utxo)
        try:
            block_daa = int(entry.get("blockDaaScore") or entry.get("block_daa_score") or 0)
            amount = int(entry.get("amount") or 0)
            index = int(out.get("index") or 0)
        except (TypeError, ValueError):
            continue
        conf = daa_confirmations(virtual_daa_score, block_daa)
        if index != output_index or amount != amount_sompi or conf < need:
            continue
        return {
            "tx_id": txid,
            "output_index": index,
            "amount_sompi": amount,
            "block_daa_score": block_daa,
            "confirmations": conf,
        }
    return None
