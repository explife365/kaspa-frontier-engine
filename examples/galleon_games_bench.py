"""Run many Galleon DeFi game transactions and collect a results bundle.

  python examples/galleon_games_bench.py --count 1000 --broadcast
  python examples/galleon_games_bench.py --count 50 --dry-run
  python examples/galleon_games_bench.py --seed-bankroll --broadcast

Requires deployed games (galleon_games_deploy.py --broadcast).
Coin flip / dice need gTEST bankroll on the contract (use --seed-bankroll once).
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

from eth_utils import keccak

ROOT = Path(__file__).resolve().parents[1]
LOCAL = ROOT / ".local"
RESULTS_PATH = LOCAL / "galleon-games-bench.json"

sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from erc20 import decode_uint256, eth_call, token_balance, token_meta  # noqa: E402
from galleon import GALLEON_CHAIN_ID, GALLEON_EXPLORER, GALLEON_GTEST, GALLEON_RPC  # noqa: E402
from galleon_faucet import address_of, galleon_key, require_galleon_chain  # noqa: E402
from galleon_games_common import APPROVE_GAS, PLAY_GAS, game_address, parse_bet  # noqa: E402
from eth_account import Account
from eth_utils import to_checksum_address

from galleon_faucet import rpc_hex  # noqa: E402
from galleon import fits_balance, GALLEON_MIN_GAS_WEI  # noqa: E402
from galleon_pool_seed import encode_approve, send_contract  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402

SELECTOR_FLIP = "0x" + keccak(text="flip(bool,uint256)").hex()[:8]
SELECTOR_ROLL = "0x" + keccak(text="rollDice(uint8,uint256)").hex()[:8]
SELECTOR_BUY = "0x" + keccak(text="buyTicket()").hex()[:8]
SELECTOR_GAMES = "0x" + keccak(text="gamesPlayed()").hex()[:8]
SELECTOR_TRANSFER = "0xa9059cbb"

BANKROLL_GTEST = 500.0
MIN_BET = "0.1"
TICKET_BET = "0.5"
CHECKPOINT_EVERY = 50
# Igra prepaid check: balance >= gasLimit * gasPrice. Keep under wallet balance headroom.
PLAY_GAS_LIMIT = 90_000


@dataclass
class TxResult:
    index: int
    game: str
    ok: bool
    tx_hash: str | None
    error: str | None
    latency_ms: int
    detail: str


def encode_flip(heads: bool, wager: int) -> str:
    return SELECTOR_FLIP + ("0" * 63 + ("1" if heads else "0")) + f"{wager:064x}"


def encode_roll(guess: int, wager: int) -> str:
    return SELECTOR_ROLL + f"{guess:064x}" + f"{wager:064x}"


def encode_transfer(to: str, amount: int) -> str:
    return SELECTOR_TRANSFER + to.lower().removeprefix("0x").zfill(64) + f"{amount:064x}"


def gas_limit_for_balance(balance: int, requested: int = PLAY_GAS_LIMIT) -> int:
    """Pick the largest gas limit that still passes Igra's prepaid balance check."""
    cap = max(55_000, balance // GALLEON_MIN_GAS_WEI - 2_000)
    return max(55_000, min(requested, cap))


def send_quiet(key: str, to: str, data: str, gas: int, value: int = 0) -> str:
    src = address_of(key)
    bal = int(rpc_hex("eth_getBalance", [src, "latest"]), 16)
    gas = gas_limit_for_balance(bal, gas)
    if not fits_balance(bal, gas, GALLEON_MIN_GAS_WEI, value):
        need = gas * GALLEON_MIN_GAS_WEI + value
        raise RuntimeError(f"would be dropped on Igra: have {bal} wei, need {need}")
    nonce = int(rpc_hex("eth_getTransactionCount", [src, "pending"]), 16)
    tx = {
        "chainId": GALLEON_CHAIN_ID,
        "nonce": nonce,
        "to": to_checksum_address(to),
        "value": value,
        "gas": gas,
        "gasPrice": GALLEON_MIN_GAS_WEI,
        "data": data,
    }
    signed = Account.sign_transaction(tx, key)
    raw = signed.raw_transaction.hex()
    if not raw.startswith("0x"):
        raw = "0x" + raw
    return rpc_hex("eth_sendRawTransaction", [raw])


def setup_approvals(flip_addr: str, dice_addr: str, jackpot_addr: str, broadcast: bool) -> None:
    key = galleon_key()
    owner = address_of(key)
    max_uint = (1 << 256) - 1
    for label, spender in (
        ("coin_flip", flip_addr),
        ("dice", dice_addr),
        ("jackpot", jackpot_addr),
    ):
        from galleon_pool_seed import allowance_of

        if allowance_of(GALLEON_GTEST, owner, spender) >= max_uint // 2:
            continue
        data = encode_approve(spender, max_uint)
        print(f"approve gTEST -> {label}")
        if broadcast:
            send_quiet(key, GALLEON_GTEST, data, APPROVE_GAS, 0)


def seed_bankroll(broadcast: bool) -> dict[str, str]:
    key = galleon_key()
    amount = parse_bet(str(BANKROLL_GTEST))
    targets = {
        "coin_flip": game_address("GALLEON_COIN_FLIP"),
        "dice": game_address("GALLEON_DICE"),
    }
    txs: dict[str, str] = {}
    for label, addr in targets.items():
        bal = token_balance(GALLEON_RPC, GALLEON_GTEST, addr)
        if bal >= amount:
            print(f"{label} bankroll already {bal / 1e18:.2f} gTEST — skip")
            continue
        data = encode_transfer(addr, amount)
        print(f"seed {label}  {BANKROLL_GTEST} gTEST -> {addr}")
        if broadcast:
            txs[label] = send_quiet(key, GALLEON_GTEST, data, 65_000, 0)
        else:
            print("  dry-run transfer OK")
    return txs


def read_games_played(flip_addr: str) -> int:
    return decode_uint256(eth_call(GALLEON_RPC, flip_addr, SELECTOR_GAMES))


def plan_games(count: int) -> list[str]:
    """Mix: ~45% coin flip, ~45% dice, ~10% jackpot tickets."""
    flip_n = count * 45 // 100
    dice_n = count * 45 // 100
    jackpot_n = count - flip_n - dice_n
    return ["coin_flip"] * flip_n + ["dice"] * dice_n + ["jackpot"] * jackpot_n


def play_one(
    index: int,
    game: str,
    *,
    flip_addr: str,
    dice_addr: str,
    jackpot_addr: str,
    wager: int,
    ticket_price: int,
    broadcast: bool,
) -> TxResult:
    key = galleon_key()
    started = time.perf_counter()
    detail = ""
    try:
        if game == "coin_flip":
            heads = index % 2 == 0
            detail = f"{'heads' if heads else 'tails'} {MIN_BET} gTEST"
            data = encode_flip(heads, wager)
            target = flip_addr
        elif game == "dice":
            guess = (index % 6) + 1
            detail = f"guess {guess} {MIN_BET} gTEST"
            data = encode_roll(guess, wager)
            target = dice_addr
        elif game == "jackpot":
            detail = f"ticket {TICKET_BET} gTEST"
            data = SELECTOR_BUY
            target = jackpot_addr
        else:
            raise RuntimeError(f"unknown game {game}")

        if broadcast:
            tx_hash = send_quiet(key, target, data, PLAY_GAS_LIMIT, 0)
            latency = int((time.perf_counter() - started) * 1000)
            return TxResult(index, game, True, tx_hash, None, latency, detail)
        latency = int((time.perf_counter() - started) * 1000)
        return TxResult(index, game, True, None, None, latency, f"dry-run {detail}")
    except Exception as err:  # noqa: BLE001
        latency = int((time.perf_counter() - started) * 1000)
        return TxResult(index, game, False, None, str(err), latency, detail)


def summarize(results: list[TxResult]) -> dict[str, Any]:
    ok = [r for r in results if r.ok]
    fail = [r for r in results if not r.ok]
    by_game: dict[str, dict[str, int]] = {}
    latencies = [r.latency_ms for r in ok if r.latency_ms]
    for r in results:
        bucket = by_game.setdefault(r.game, {"ok": 0, "fail": 0})
        bucket["ok" if r.ok else "fail"] += 1
    return {
        "total": len(results),
        "ok": len(ok),
        "fail": len(fail),
        "by_game": by_game,
        "latency_ms": {
            "p50": sorted(latencies)[len(latencies) // 2] if latencies else 0,
            "max": max(latencies) if latencies else 0,
            "avg": int(sum(latencies) / len(latencies)) if latencies else 0,
        },
    }


def write_results(body: dict[str, Any]) -> Path:
    LOCAL.mkdir(parents=True, exist_ok=True)
    RESULTS_PATH.write_text(json.dumps(body, indent=2) + "\n", encoding="utf-8")
    return RESULTS_PATH


TN10_WALLETS = ("alice", "bob", "carol", "dave", "eve", "frank", "grace")
TN10_KAS = 1.0
MAX_TN10_FAILS = 25


async def run_tn10_remainder(
    results: list[TxResult],
    count: int,
    started_at: int,
    flip_addr: str,
    start_bal: int,
    start_played: int,
    owner: str,
) -> list[TxResult]:
    os.environ.pop("KASPA_RPC_URL", None)
    from tn10_transfer import send_kas  # noqa: E402

    index = len(results)
    names = TN10_WALLETS
    fails = 0
    while index < count:
        src = names[index % len(names)]
        dest = names[(index + 3) % len(names)]
        if src == dest:
            dest = names[(index + 4) % len(names)]
        started = time.perf_counter()
        try:
            txid = await send_kas(src, dest, TN10_KAS, conf=None)
            latency = int((time.perf_counter() - started) * 1000)
            result = TxResult(
                index + 1,
                "tn10_transfer",
                True,
                txid,
                None,
                latency,
                f"{src}->{dest} {TN10_KAS} tKAS",
            )
            fails = 0
        except Exception as err:  # noqa: BLE001
            latency = int((time.perf_counter() - started) * 1000)
            result = TxResult(
                index + 1,
                "tn10_transfer",
                False,
                None,
                str(err),
                latency,
                f"{src}->{dest}",
            )
            fails += 1
        results.append(result)
        index += 1
        mark = "ok" if result.ok else "FAIL"
        tx_disp = (result.tx_hash or "-")[:18]
        print(f"{index:4d}/{count}  tn10_xfer   {mark}  {result.latency_ms}ms  {tx_disp}  {result.detail}")
        if not result.ok:
            print(f"         {result.error}")
            if fails >= MAX_TN10_FAILS:
                print(f"stopping: {MAX_TN10_FAILS} consecutive TN10 failures")
                break
            continue
        if index % CHECKPOINT_EVERY == 0:
            body = build_report(
                results,
                started_at=started_at,
                start_bal=start_bal,
                end_bal=token_balance(GALLEON_RPC, GALLEON_GTEST, owner),
                start_played=start_played,
                end_played=read_games_played(flip_addr),
                partial=True,
            )
            write_results(body)
    return results


def load_resume_results() -> list[TxResult]:
    if not RESULTS_PATH.is_file():
        return []
    body = json.loads(RESULTS_PATH.read_text(encoding="utf-8"))
    rows = body.get("transactions") or []
    return [TxResult(**row) for row in rows if isinstance(row, dict)]


def run_bench(
    count: int,
    broadcast: bool,
    seed: bool,
    tn10_remainder: bool,
    resume: bool,
    galleon_only: bool,
) -> dict[str, Any]:
    require_galleon_chain()
    load_kaspa_env(ROOT)
    flip_addr = game_address("GALLEON_COIN_FLIP")
    dice_addr = game_address("GALLEON_DICE")
    jackpot_addr = game_address("GALLEON_JACKPOT")
    wager = parse_bet(MIN_BET)
    ticket_price = parse_bet(TICKET_BET)

    owner = address_of(galleon_key())
    meta = token_meta(GALLEON_RPC, GALLEON_GTEST)
    start_bal = token_balance(GALLEON_RPC, GALLEON_GTEST, owner)
    start_played = read_games_played(flip_addr)

    if seed:
        seed_bankroll(broadcast)
    if broadcast:
        setup_approvals(flip_addr, dice_addr, jackpot_addr, True)

    prior = load_resume_results() if resume else []
    if prior:
        print(f"resume  {len(prior)} prior txs from {RESULTS_PATH}")
    games = plan_games(count)
    if prior and len(prior) >= count:
        games = []
    elif prior:
        # Resume skips remaining Galleon schedule; TN10 remainder fills the count.
        games = []
    results: list[TxResult] = list(prior)
    started_at = int(time.time())
    if prior and RESULTS_PATH.is_file():
        started_at = int(json.loads(RESULTS_PATH.read_text(encoding="utf-8")).get("started_at", started_at))

    print(f"\n=== Galleon games bench ({count} txs) ===")
    print(f"chain {GALLEON_CHAIN_ID}  wallet {owner}")
    print(f"gTEST {start_bal / 10**meta['decimals']:.4f}  coin_flip rounds {start_played}")
    print(f"mode  {'broadcast' if broadcast else 'dry-run'}\n")

    for offset, game in enumerate(games, start=len(results) + 1):
        i = offset
        result = play_one(
            i,
            game,
            flip_addr=flip_addr,
            dice_addr=dice_addr,
            jackpot_addr=jackpot_addr,
            wager=wager,
            ticket_price=ticket_price,
            broadcast=broadcast,
        )
        results.append(result)
        mark = "ok" if result.ok else "FAIL"
        tx = result.tx_hash or "-"
        tx_disp = (result.tx_hash or "-")[:18]
        print(f"{i:4d}/{count}  {game:10}  {mark}  {result.latency_ms}ms  {tx_disp}  {result.detail}")
        if not result.ok:
            print(f"         {result.error}")
            if broadcast and result.error and "would be dropped on Igra" in result.error:
                print("stopping: iKAS gas floor hit (drip, sweep extras, or L1 entry)")
                break

        if i % CHECKPOINT_EVERY == 0:
            body = build_report(
                results,
                started_at=started_at,
                start_bal=start_bal,
                end_bal=token_balance(GALLEON_RPC, GALLEON_GTEST, owner),
                start_played=start_played,
                end_played=read_games_played(flip_addr),
                partial=True,
            )
            write_results(body)

    if broadcast and tn10_remainder and not galleon_only and len(results) < count:
        print(f"\n--- TN10 remainder ({count - len(results)} txs) ---")
        results = asyncio.run(
            run_tn10_remainder(
                results,
                count,
                started_at,
                flip_addr,
                start_bal,
                start_played,
                owner,
            )
        )

    end_bal = token_balance(GALLEON_RPC, GALLEON_GTEST, owner)
    end_played = read_games_played(flip_addr)
    report = build_report(
        results,
        started_at=started_at,
        start_bal=start_bal,
        end_bal=end_bal,
        start_played=start_played,
        end_played=end_played,
        partial=False,
    )
    path = write_results(report)
    report["results_path"] = str(path)
    return report


def build_report(
    results: list[TxResult],
    *,
    started_at: int,
    start_bal: int,
    end_bal: int,
    start_played: int,
    end_played: int,
    partial: bool,
) -> dict[str, Any]:
    return {
        "network": "galleon-testnet",
        "chain_id": GALLEON_CHAIN_ID,
        "not_mainnet": True,
        "partial": partial,
        "started_at": started_at,
        "finished_at": int(time.time()),
        "wallet": address_of(galleon_key()),
        "gtest_delta": (end_bal - start_bal) / 1e18,
        "coin_flip_rounds_delta": end_played - start_played,
        "summary": summarize(results),
        "transactions": [asdict(r) for r in results],
        "explorer": GALLEON_EXPLORER,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Galleon DeFi games batch bench")
    parser.add_argument("--count", type=int, default=1000, help="number of game txs")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--broadcast", action="store_true")
    parser.add_argument("--seed-bankroll", action="store_true", help="fund flip/dice contracts")
    parser.add_argument(
        "--tn10-remainder",
        action="store_true",
        default=True,
        help="after Galleon gas runs out, finish count with TN10 wallet transfers (default on)",
    )
    parser.add_argument("--no-tn10-remainder", action="store_false", dest="tn10_remainder")
    parser.add_argument("--galleon-only", action="store_true", help="skip TN10 remainder phase")
    parser.add_argument("--resume", action="store_true", help="continue from .local/galleon-games-bench.json")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    if not args.dry_run and not args.broadcast and not args.seed_bankroll:
        parser.error("pass --dry-run, --broadcast, or --seed-bankroll")
    if args.count < 1:
        parser.error("--count must be >= 1")

    if args.seed_bankroll and not args.broadcast and not args.dry_run:
        load_kaspa_env(ROOT)
        require_galleon_chain()
        seed_bankroll(False)
        return 0

    if args.galleon_only:
        args.tn10_remainder = False
    report = run_bench(
        args.count,
        broadcast=args.broadcast,
        seed=args.seed_bankroll,
        tn10_remainder=args.tn10_remainder,
        resume=args.resume,
        galleon_only=args.galleon_only,
    )
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        s = report["summary"]
        print(f"\nsummary  {s['ok']}/{s['total']} ok  fail {s['fail']}")
        print(f"by_game  {s['by_game']}")
        print(f"latency  avg {s['latency_ms']['avg']}ms  p50 {s['latency_ms']['p50']}ms")
        print(f"gTEST delta  {report['gtest_delta']:.4f}")
        print(f"results  {report.get('results_path', RESULTS_PATH)}")
    return 0 if report["summary"]["fail"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
