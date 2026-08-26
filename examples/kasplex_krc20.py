"""Kasplex KRC-20 on TN10: indexer lookup + commit-reveal mint.

  python examples/kasplex_krc20.py
  python examples/kasplex_krc20.py --address alice
  python examples/kasplex_krc20.py --token TMBMN
  python examples/kasplex_krc20.py --commit-address --tick TMBMN --from alice
  python examples/kasplex_krc20.py --mint TMBMN --from alice
  python examples/kasplex_krc20.py --transfer TMBMN --from alice --to bob --amt 50000000000

This is Kasplex inscriptions, not USD and not L1 EVM.
TMBMN is this crate's live TN10 KRC-20 (Frontier): minted and transferred here.
A new crate-owned tick needs the 1000 tKAS Kasplex deploy burn (named wallets do not have it).
Mint burns ~1 tKAS protocol fee. Transfer does not. Keys stay in kaspa.env.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import time
from pathlib import Path

from kaspa import (
    Address,
    Opcodes,
    PaymentOutput,
    Resolver,
    RpcClient,
    ScriptBuilder,
    address_from_script_public_key,
    create_transactions,
)

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from kaspa_env import load_kaspa_env  # noqa: E402
from tn10_rest import (  # noqa: E402
    GET_ATTEMPTS,
    MIN_PRIORITY_FEE_SOMPI,
    MIN_STORAGE_SAFE_SOMPI,
    address_balance_sompi,
    decode_json_body,
    encode_path_segment,
    https_get,
    retry_after_wait,
    select_commit_entries,
    spendable_entries,
    virtual_daa,
)
from tn10_wallets import (  # noqa: E402
    DEV_DONATION_ADDRESS,
    EXPLORER,
    WALLET_NAMES,
    address_from_key,
    get_wallet,
    key_for,
)

NETWORK_ID = "testnet-10"
KASPLEX = "https://tn10api.kasplex.org/v1"
USER_AGENT = "kaspa-frontier-engine/0.3"
FRONTIER_TICK = "TMBMN"
FRONTIER_NAME = "Frontier"
MINT_FEE_SOMPI = 100_000_000
DEPLOY_FEE_SOMPI = 1_000 * 100_000_000
COMMIT_SOMPI = 100_000_000
TRANSFER_COMMIT_SOMPI = MIN_STORAGE_SAFE_SOMPI
ACCEPT_TIMEOUT_S = 90
load_kaspa_env(ROOT)
RPC_URL = (os.environ.get("KASPA_RPC_URL") or "").strip() or None


def kasplex_tick(tick: str) -> str:
    ticker = tick.strip().lower()
    if not (
        4 <= len(ticker) <= 6
        and ticker.isascii()
        and ticker.isalnum()
    ):
        raise ValueError("Kasplex tick must be 4-6 ASCII alphanumeric characters")
    return ticker


def deploy_inscription(tick: str, max_supply: str, lim: str, dec: str = "8") -> str:
    ticker = kasplex_tick(tick)
    cap = max_supply.strip()
    limit = lim.strip()
    decimals = dec.strip()
    if not cap.isdigit() or int(cap) <= 0:
        raise ValueError("deploy max must be a positive integer")
    if not limit.isdigit() or int(limit) <= 0:
        raise ValueError("deploy lim must be a positive integer")
    if int(limit) > int(cap):
        raise ValueError("deploy lim must not exceed max")
    if not decimals.isdigit() or int(decimals) > 18:
        raise ValueError("deploy dec must be 0-18")
    return (
        f'{{"p":"krc-20","op":"deploy","tick":"{ticker}",'
        f'"max":"{cap}","lim":"{limit}","dec":"{decimals}"}}'
    )


def mint_inscription(tick: str) -> str:
    ticker = kasplex_tick(tick)
    return f'{{"p":"krc-20","op":"mint","tick":"{ticker}"}}'


def validate_mint_row(token: str, row: dict) -> int:
    try:
        returned = kasplex_tick(str(row.get("tick") or "")).upper()
    except ValueError as error:
        raise RuntimeError("Kasplex returned an invalid ticker") from error
    if returned != token:
        raise RuntimeError(f"Kasplex returned ticker {returned}, expected {token}")
    try:
        maximum = int(row["max"])
        minted = int(row["minted"])
        limit = int(row["lim"])
    except (KeyError, TypeError, ValueError) as error:
        raise RuntimeError(f"Kasplex returned malformed limits for {token}") from error
    if maximum <= 0 or limit <= 0 or limit > maximum:
        raise RuntimeError(f"Kasplex returned invalid lim/max for {token}")
    if minted < 0 or minted >= maximum or maximum - minted < limit:
        raise RuntimeError(f"{token} has insufficient remaining supply for one mint limit")
    return limit


def transfer_inscription(tick: str, amt: str, to: str) -> str:
    ticker = kasplex_tick(tick)
    amount = amt.strip()
    dest = to.strip().lower()
    if not amount.isdigit() or int(amount) <= 0:
        raise ValueError("transfer amt must be a positive integer (including decimals)")
    try:
        dest = Address(dest).to_string().lower()
    except Exception as err:
        raise ValueError("invalid Kaspa transfer destination") from err
    if not dest.startswith("kaspatest:"):
        raise ValueError("TN10 transfer destination must be a parsed kaspatest address")
    return f'{{"p":"krc-20","op":"transfer","tick":"{ticker}","amt":"{amount}","to":"{dest}"}}'


def kasplex_get(path: str, timeout_s: float = 12.0) -> dict:
    if not KASPLEX.startswith("https://"):
        raise RuntimeError("Kasplex endpoint must be https")
    url = f"{KASPLEX}{path}"
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
                last = RuntimeError(f"Kasplex {code} for {url}")
                time.sleep(retry_after_wait(retry_after, attempt))
                continue
            if code >= 400:
                raise RuntimeError(f"Kasplex {code} for {url}")
            body = decode_json_body(raw, encoding)
            if not isinstance(body, dict):
                raise RuntimeError(f"unexpected Kasplex body for {url}")
            return body
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
                raise RuntimeError(f"Kasplex timeout/connect for {url}") from err
            time.sleep(0.15 * (2**attempt))
    raise RuntimeError(f"Kasplex failed for {url}: {last}")


def kasplex_address_tokenlist(address: str) -> dict:
    encoded = f"/krc20/address/{encode_path_segment(address)}/tokenlist"
    try:
        return kasplex_get(encoded)
    except RuntimeError as err:
        if any(f"Kasplex {code}" in str(err) for code in (400, 403, 404)):
            return kasplex_get(f"/krc20/address/{address}/tokenlist")
        raise


def redeem_script(xonly_hex: str, payload: str) -> ScriptBuilder:
    builder = ScriptBuilder()
    builder.add_data(xonly_hex)
    builder.add_op(Opcodes.OpCheckSig)
    builder.add_op(Opcodes.OpFalse)
    builder.add_op(Opcodes.OpIf)
    builder.add_data(b"kasplex")
    builder.add_i64(0)
    builder.add_data(payload.encode())
    builder.add_op(Opcodes.OpEndIf)
    return builder


def commit_address(xonly_hex: str, payload: str) -> tuple[ScriptBuilder, str]:
    script = redeem_script(xonly_hex, payload)
    spk = script.create_pay_to_script_hash_script()
    addr = str(address_from_script_public_key(spk, "testnet"))
    if not addr.startswith("kaspatest:"):
        raise RuntimeError(f"commit address is not testnet: {addr}")
    return script, addr


def _as_bytes(value) -> bytes:
    if isinstance(value, bytes):
        return value
    if isinstance(value, bytearray):
        return bytes(value)
    text = str(value)
    try:
        return bytes.fromhex(text)
    except ValueError:
        return text.encode()


def _outpoint_id(inp) -> tuple[str, int]:
    prev = getattr(inp, "previous_outpoint", None)
    if prev is None:
        return ("", -1)
    txid = str(getattr(prev, "transaction_id", "") or "")
    index = int(getattr(prev, "index", -1))
    return (txid, index)


def fill_p2sh_reveal(
    pending, script: ScriptBuilder, commit_txid: str, commit_index: int, key
) -> None:
    """Overwrite only the P2SH commit input. Other outputs of the same tx (change) stay P2PKH."""
    pending.sign([key], check_fully_signed=False)
    filled_any = False
    for index, inp in enumerate(pending.transaction.inputs):
        if _outpoint_id(inp) != (commit_txid, commit_index):
            continue
        signature = pending.create_input_signature(index, key)
        encoded = script.encode_pay_to_script_hash_signature_script(_as_bytes(signature))
        pending.fill_input(index, _as_bytes(encoded))
        filled_any = True
    if not filled_any:
        raise RuntimeError(f"reveal tx does not spend commit {commit_txid}:{commit_index}")


def wait_for_kasplex_op(reveal_id: str) -> dict:
    deadline = time.time() + ACCEPT_TIMEOUT_S
    url_path = f"/krc20/op/{reveal_id}"
    while time.time() < deadline:
        try:
            last = kasplex_get(url_path)
        except RuntimeError as err:
            # Indexer often 403/404 until the reveal is visible.
            if "Kasplex 403" in str(err) or "Kasplex 404" in str(err):
                time.sleep(3)
                continue
            raise
        rows = last.get("result") or []
        if rows:
            row = rows[0]
            accept = str(row.get("opAccept") or "")
            if accept == "1" or accept.lower() == "true":
                print(f"Kasplex opAccept=1  {row.get('op')}  {row.get('tick')}")
                return row
            err = row.get("opError")
            if err:
                raise RuntimeError(f"Kasplex rejected op: {err}")
        time.sleep(3)
    raise RuntimeError(f"Kasplex did not index {reveal_id} in {ACCEPT_TIMEOUT_S}s")


async def make_client() -> RpcClient:
    if RPC_URL:
        client = RpcClient(url=RPC_URL, network_id=NETWORK_ID)
        print(f"RPC {RPC_URL}")
    else:
        client = RpcClient(resolver=Resolver(), network_id=NETWORK_ID)
        print("RPC Resolver (public TN10)")
    await client.connect(strategy="fallback")
    return client


async def wait_for_output(client: RpcClient, address: str, txid: str) -> dict:
    deadline = time.monotonic() + ACCEPT_TIMEOUT_S
    dest = Address(address)
    while True:
        result = await client.get_utxos_by_addresses({"addresses": [dest]})
        for entry in result["entries"]:
            if entry["outpoint"]["transactionId"] == txid:
                return entry
        if time.monotonic() >= deadline:
            raise TimeoutError(f"txid {txid} not in UTXO set for {address}")
        await asyncio.sleep(1.5)


def print_mintable() -> None:
    info = kasplex_get("/info")
    page = kasplex_get("/krc20/tokenlist")
    print(f"Kasplex {KASPLEX}")
    print(f"indexer  {info.get('message')}  tokens={info.get('result', {}).get('tokenTotal')}")
    rows = page.get("result") or []
    open_mints = [
        row
        for row in rows
        if row.get("mod") == "mint"
        and row.get("state") == "deployed"
        and row.get("tick")
        and int(row.get("max") or 0) > int(row.get("minted") or 0)
    ]
    print(f"first page  {len(rows)} tokens  {len(open_mints)} open mints")
    for row in open_mints[:12]:
        print(
            f"  {row['tick']}  minted={row.get('minted')}  lim={row.get('lim')}  "
            f"{EXPLORER}/txs/{row.get('hashRev')}"
        )


def print_address_tokens(name: str) -> None:
    wallet = get_wallet(name, ROOT)
    body = kasplex_address_tokenlist(wallet.address)
    rows = body.get("result") or []
    print(f"{name}  {wallet.address}")
    if not rows:
        print("  (no KRC-20 balances)")
        return
    for row in rows:
        print(f"  {row.get('tick') or row.get('ca')}  {row.get('balance')}")


def print_token(tick: str) -> None:
    ticker = kasplex_tick(tick).upper()
    body = kasplex_get(f"/krc20/token/{ticker}")
    rows = body.get("result") or []
    if not rows:
        print(f"no token {ticker}")
        return
    row = rows[0]
    name = FRONTIER_NAME if ticker == FRONTIER_TICK else ticker
    print(f"{name}  tick={ticker}  holders={row.get('holderTotal')}  minted={row.get('minted')}/{row.get('max')}")
    print(json.dumps(row, indent=2))
    print("envelope", mint_inscription(ticker))


async def show_commit_address(src_name: str, tick: str) -> None:
    payload = mint_inscription(tick)
    key = key_for(src_name)
    xonly = key.to_public_key().to_x_only_public_key().to_string()
    script, p2sh = commit_address(xonly, payload)
    print(f"from     {address_from_key(key)}")
    print(f"tick     {kasplex_tick(tick)}")
    print(f"envelope {payload}")
    print(f"commit   {p2sh}")
    print(f"script   {script.to_string()[:80]}...")
    print("This does not submit. Mint burns ~1 tKAS: --mint TICK --from alice")


async def commit_and_reveal(
    src_name: str,
    payload: str,
    commit_sompi: int,
    reveal_fee: int,
) -> str:
    src = get_wallet(src_name, ROOT)
    key = key_for(src.name)
    from_addr = address_from_key(key)
    if from_addr != src.address:
        raise RuntimeError("wallet address mismatch")
    xonly = key.to_public_key().to_x_only_public_key().to_string()
    script, p2sh = commit_address(xonly, payload)

    client = await make_client()
    print("connected")
    print(f"envelope {payload}")
    print(f"commit   {p2sh}")
    try:
        utxos = await client.get_utxos_by_addresses({"addresses": [from_addr]})
        mature = spendable_entries(utxos.get("entries") or [], virtual_daa())
        selected, commit_sompi = select_commit_entries(mature, commit_sompi)
        print(f"commit amount {commit_sompi} sompi  ({len(selected)} inputs)")
        built = create_transactions(
            network_id=NETWORK_ID,
            entries=selected,
            change_address=from_addr,
            outputs=[PaymentOutput(Address(p2sh), commit_sompi)],
            priority_fee=MIN_PRIORITY_FEE_SOMPI,
        )
        pending_list = built["transactions"]
        commit_id = ""
        for pending in pending_list:
            pending.sign([key])
            commit_id = await pending.submit(client)
            print(f"commit tx  {commit_id}")
            print(f"  {EXPLORER}/txs/{commit_id}")
        commit_entry = await wait_for_output(client, p2sh, commit_id)
        print("commit accepted, revealing")
        return await submit_reveal(
            client, key, from_addr, script, commit_entry, reveal_fee
        )
    finally:
        await client.disconnect()


async def mint(src_name: str, tick: str) -> str:
    token = kasplex_tick(tick).upper()
    info = kasplex_get(f"/krc20/token/{token}")
    rows = info.get("result") or []
    if not rows:
        raise RuntimeError(f"Kasplex has no token {token}")
    row = rows[0]
    if row.get("mod") != "mint" or row.get("state") != "deployed":
        raise RuntimeError(f"{token} is not an open mint ({row.get('mod')}/{row.get('state')})")
    validate_mint_row(token, row)
    reveal_id = await commit_and_reveal(
        src_name, mint_inscription(token), COMMIT_SOMPI, MINT_FEE_SOMPI
    )
    wait_for_kasplex_op(reveal_id)
    print_address_tokens(src_name)
    return reveal_id


async def deploy(src_name: str, tick: str, max_supply: str, lim: str) -> str:
    token = kasplex_tick(tick).upper()
    src = get_wallet(src_name, ROOT)
    have = address_balance_sompi(src.address) or 0
    if have < DEPLOY_FEE_SOMPI + MIN_STORAGE_SAFE_SOMPI:
        need = DEPLOY_FEE_SOMPI / 100_000_000
        got = have / 100_000_000
        raise RuntimeError(
            f"Kasplex deploy of {token} burns {need:.0f} tKAS; {src.name} has {got:.4f} tKAS. "
            f"Live crate token stays {FRONTIER_TICK} ({FRONTIER_NAME})."
        )
    info = kasplex_get(f"/krc20/token/{token}")
    rows = info.get("result") or []
    if rows:
        state = (rows[0].get("state") or "").lower()
        if state not in ("", "unused"):
            raise RuntimeError(f"{token} already deployed ({state})")
    payload = deploy_inscription(token, max_supply, lim)
    print(f"deploy {token}  max={max_supply} lim={lim}  burn {DEPLOY_FEE_SOMPI} sompi")
    reveal_id = await commit_and_reveal(
        src_name, payload, COMMIT_SOMPI, DEPLOY_FEE_SOMPI
    )
    wait_for_kasplex_op(reveal_id)
    print_token(token)
    return reveal_id


def token_balance(address: str, tick: str) -> int:
    token = kasplex_tick(tick).upper()
    held = kasplex_address_tokenlist(address)
    for row in held.get("result") or []:
        if (row.get("tick") or "").upper() == token:
            return int(row.get("balance") or 0)
    return 0


async def distribute(src_name: str, tick: str, amt: str) -> list[str]:
    """Send Frontier to named wallets that hold none. Makes TMBMN a 5-holder crate token."""
    token = kasplex_tick(tick).upper()
    src = get_wallet(src_name, ROOT)
    amount = int(amt)
    reveals: list[str] = []
    for dest_name in WALLET_NAMES:
        dest = get_wallet(dest_name, ROOT)
        if dest.address == src.address:
            continue
        if token_balance(dest.address, token) > 0:
            print(f"skip {dest_name} (already holds {token})")
            continue
        print(f"distribute {amount} {token} -> {dest_name}")
        reveals.append(await transfer(src_name, dest_name, token, str(amount)))
    if not reveals:
        print(f"no new {token} holders; crate wallets already funded")
    return reveals


async def transfer(src_name: str, dest_name: str, tick: str, amt: str) -> str:
    token = kasplex_tick(tick).upper()
    src = get_wallet(src_name, ROOT)
    dest = get_wallet(dest_name, ROOT)
    if src.address == dest.address:
        raise ValueError("refusing self-transfer")
    held = kasplex_address_tokenlist(src.address)
    rows = held.get("result") or []
    balance = 0
    for row in rows:
        if (row.get("tick") or "").upper() == token:
            balance = int(row.get("balance") or 0)
            break
    amount = int(amt)
    if amount <= 0 or amount > balance:
        raise RuntimeError(f"{src.name} has {balance} {token}, cannot send {amount}")
    payload = transfer_inscription(token, str(amount), dest.address)
    print(f"{src.name} -> {dest.name}  {amount} {token}  (Kasplex transfer, no 1 tKAS mint fee)")
    reveal_id = await commit_and_reveal(
        src_name, payload, TRANSFER_COMMIT_SOMPI, MIN_PRIORITY_FEE_SOMPI
    )
    wait_for_kasplex_op(reveal_id)
    print_address_tokens(src_name)
    print_address_tokens(dest_name)
    return reveal_id


async def submit_reveal(
    client,
    key,
    from_addr: str,
    script: ScriptBuilder,
    commit_entry: dict,
    priority_fee: int,
) -> str:
    commit_txid = commit_entry["outpoint"]["transactionId"]
    commit_index = int(commit_entry["outpoint"]["index"])
    wallet_utxos = await client.get_utxos_by_addresses({"addresses": [from_addr]})
    extra = [
        entry
        for entry in spendable_entries(wallet_utxos.get("entries") or [], virtual_daa())
        if not (
            entry["outpoint"]["transactionId"] == commit_txid
            and int(entry["outpoint"]["index"]) == commit_index
        )
    ]
    fee = max(int(priority_fee), MIN_PRIORITY_FEE_SOMPI)
    reveal_built = create_transactions(
        network_id=NETWORK_ID,
        entries=extra,
        priority_entries=[commit_entry],
        change_address=from_addr,
        outputs=[],
        priority_fee=fee,
    )
    reveal_id = ""
    for pending in reveal_built["transactions"]:
        fill_p2sh_reveal(pending, script, commit_txid, commit_index, key)
        reveal_id = await pending.submit(client)
        print(f"reveal tx  {reveal_id}")
        print(f"  {EXPLORER}/txs/{reveal_id}")
        print(f"  Kasplex  {KASPLEX}/krc20/op/{reveal_id}")
    if not reveal_id:
        raise RuntimeError("reveal produced no transaction")
    return reveal_id


async def reveal_existing(
    src_name: str,
    tick: str,
    commit_txid: str | None,
    dest_name: str | None = None,
    amt: str | None = None,
) -> str:
    if dest_name and amt:
        dest = get_wallet(dest_name, ROOT)
        payload = transfer_inscription(tick, amt, dest.address)
        reveal_fee = MIN_PRIORITY_FEE_SOMPI
    else:
        payload = mint_inscription(tick)
        reveal_fee = MINT_FEE_SOMPI
    src = get_wallet(src_name, ROOT)
    key = key_for(src.name)
    from_addr = address_from_key(key)
    xonly = key.to_public_key().to_x_only_public_key().to_string()
    script, p2sh = commit_address(xonly, payload)
    client = await make_client()
    print("connected")
    print(f"envelope {payload}")
    print(f"commit   {p2sh}")
    try:
        result = await client.get_utxos_by_addresses({"addresses": [Address(p2sh)]})
        entries = result.get("entries") or []
        if commit_txid:
            entries = [e for e in entries if e["outpoint"]["transactionId"] == commit_txid]
        if not entries:
            raise RuntimeError(
                f"no P2SH UTXO at {p2sh}. Pass the commit txid if the indexer lagged."
            )
        commit_entry = max(entries, key=lambda e: int(e["utxoEntry"]["amount"]))
        print(f"spending  {commit_entry['outpoint']['transactionId']}")
        reveal_id = await submit_reveal(
            client, key, from_addr, script, commit_entry, reveal_fee
        )
        wait_for_kasplex_op(reveal_id)
        print_address_tokens(src_name)
        if dest_name:
            print_address_tokens(dest_name)
        return reveal_id
    finally:
        await client.disconnect()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Kasplex KRC-20 on TN10")
    parser.add_argument("--address", choices=WALLET_NAMES)
    parser.add_argument("--token")
    parser.add_argument("--tick")
    parser.add_argument("--from", dest="src", choices=WALLET_NAMES, default="alice")
    parser.add_argument("--commit-address", action="store_true")
    parser.add_argument("--mint", metavar="TICK")
    parser.add_argument("--transfer", metavar="TICK")
    parser.add_argument("--to", dest="dest", choices=WALLET_NAMES)
    parser.add_argument("--amt", help="token amount including decimals, e.g. 50000000000")
    parser.add_argument("--reveal", metavar="COMMIT_TXID", nargs="?", const="")
    parser.add_argument(
        "--deploy",
        metavar="TICK",
        help="Kasplex deploy (burns 1000 tKAS). Refuses if the wallet cannot pay.",
    )
    parser.add_argument("--max", dest="max_supply", default="2100000000000000")
    parser.add_argument("--lim", default="100000000000")
    parser.add_argument(
        "--distribute",
        action="store_true",
        help=f"Send --amt of {FRONTIER_TICK} from --from to named wallets with zero balance",
    )
    return parser.parse_args()


async def main() -> None:
    args = parse_args()
    print(f"dev sig / optional mainnet KAS: {DEV_DONATION_ADDRESS}")
    if args.deploy:
        await deploy(args.src, args.deploy, args.max_supply, args.lim)
        return
    if args.distribute:
        tick = args.tick or args.token or FRONTIER_TICK
        if not args.amt:
            raise SystemExit("--distribute needs --amt")
        await distribute(args.src, tick, args.amt)
        return
    if args.mint:
        await mint(args.src, args.mint)
        return
    if args.transfer:
        if not args.dest or not args.amt:
            raise SystemExit("--transfer needs --to and --amt")
        await transfer(args.src, args.dest, args.transfer, args.amt)
        return
    if args.reveal is not None:
        tick = args.tick or args.token
        if not tick:
            raise SystemExit("--reveal needs --tick")
        await reveal_existing(
            args.src, tick, args.reveal or None, args.dest, args.amt
        )
        return
    if args.commit_address:
        tick = args.tick or args.token
        if not tick:
            raise SystemExit("--commit-address needs --tick")
        await show_commit_address(args.src, tick)
        return
    if args.address:
        print_address_tokens(args.address)
        return
    if args.token:
        print_token(args.token)
        return
    print_mintable()


if __name__ == "__main__":
    asyncio.run(main())
