"""Overnight IP-relay faucet loop — drip + sweep with optional VPN auto-rotate.

  python examples/galleon_faucet_relay.py --auto-rotate --vpn hotspot-shield
  python examples/galleon_faucet_relay.py --target-ikas 3.2 --vpn mullvad
  python examples/galleon_faucet_relay.py --rotate-cmd "powershell -File scripts/my_vpn.ps1"
  python examples/galleon_faucet_relay.py --wait-ip-change --target-ikas 3.2

State: .local/galleon-faucet-relay.json
Log:   .local/galleon-faucet-relay.log
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from galleon import GALLEON_MIN_GAS_WEI, NATIVE_TRANSFER_GAS  # noqa: E402
from galleon_faucet import (  # noqa: E402
    address_of,
    claimed_today,
    drip_with_key,
    ensure_extra_range,
    ensure_wallet,
    extra_key_env,
    faucet_blocks_connection,
    galleon_key,
    load_key,
    rpc_hex,
    send_from_key,
    wei_to_ikas,
)
from galleon_ip_rotate import fetch_egress_ip, parse_locations, rotate as rotate_vpn  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402

STATE_PATH = ROOT / ".local" / "galleon-faucet-relay.json"
LOG_PATH = ROOT / ".local" / "galleon-faucet-relay.log"


def log_line(msg: str) -> None:
    LOG_PATH.parent.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    line = f"{stamp}  {msg}\n"
    print(msg, flush=True)
    with LOG_PATH.open("a", encoding="utf-8") as handle:
        handle.write(line)


def load_state() -> dict:
    if not STATE_PATH.is_file():
        return {}
    return json.loads(STATE_PATH.read_text(encoding="utf-8"))


def save_state(body: dict) -> None:
    STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
    STATE_PATH.write_text(json.dumps(body, indent=2), encoding="utf-8")


def primary_ikas() -> float:
    dest = address_of(galleon_key())
    return wei_to_ikas(rpc_hex("eth_getBalance", [dest, "latest"]))


def sweep_wallet(index: int) -> float:
    dest = address_of(galleon_key())
    key = load_key(extra_key_env(index))
    src = address_of(key)
    from galleon_faucet import max_sendable_wei

    bal = int(rpc_hex("eth_getBalance", [src, "latest"]), 16)
    wei = max_sendable_wei(bal, NATIVE_TRANSFER_GAS, GALLEON_MIN_GAS_WEI)
    if wei <= 0:
        log_line(f"wallet{index}  no sendable iKAS to sweep")
        return 0.0
    before = primary_ikas()
    send_from_key(key, dest, wei, GALLEON_MIN_GAS_WEI)
    time.sleep(3)
    after = primary_ikas()
    return max(0.0, after - before)


def next_unclaimed_index(start: int, end: int) -> int | None:
    for index in range(start, end + 1):
        ensure_extra_range(index, index, list_existing=False)
        addr = address_of(load_key(extra_key_env(index)))
        if not claimed_today(addr):
            return index
    return None


def drip_cycle(index: int) -> str:
    """Return: ok | claimed | connection_cap | error"""
    key = load_key(extra_key_env(index))
    try:
        if drip_with_key(key, f"wallet{index}", connection_spent=False):
            return "ok"
        return "claimed"
    except RuntimeError as err:
        if faucet_blocks_connection(str(err)) or "429" in str(err):
            return "connection_cap"
        raise


def wait_for_ip_change(last_ip: str, poll_seconds: float) -> str:
    log_line(f"waiting for IP change (last={last_ip})...")
    while True:
        time.sleep(poll_seconds)
        try:
            ip = fetch_egress_ip()
        except Exception as err:  # noqa: BLE001
            log_line(f"ipify poll failed: {err}")
            continue
        if ip != last_ip:
            log_line(f"egress changed  {last_ip} -> {ip}")
            return ip
        log_line(f"waiting for IP change...  still {ip}")


def do_rotate(
    *,
    enabled: bool,
    backend: str,
    rotate_cmd: str | None,
    locations: list[str],
) -> None:
    if not enabled:
        return
    log_line(f"vpn rotate  backend={backend}")
    try:
        ip = rotate_vpn(backend=backend, custom_cmd=rotate_cmd, locations=locations)
        log_line(f"vpn egress  {ip}")
    except Exception as err:  # noqa: BLE001
        log_line(f"vpn rotate failed: {err}")


def run_relay(
    *,
    start: int,
    end: int,
    target_ikas: float,
    wait_minutes: float,
    once: bool,
    auto_rotate: bool,
    wait_ip_change: bool,
    poll_seconds: float,
    vpn_backend: str,
    rotate_cmd: str | None,
    locations: list[str],
) -> int:
    load_kaspa_env(ROOT)
    ensure_wallet()
    state = load_state()
    cursor = int(state.get("next_index", start))
    if cursor < start:
        cursor = start

    last_ip: str | None = None
    if wait_ip_change:
        try:
            last_ip = fetch_egress_ip()
            log_line(f"egress baseline  {last_ip}")
        except Exception as err:  # noqa: BLE001
            log_line(f"ipify baseline failed: {err}")

    log_line(
        f"relay start  cursor=wallet{cursor}  target={target_ikas} iKAS  "
        f"wait={wait_minutes}m  auto_rotate={auto_rotate}  wait_ip_change={wait_ip_change}  "
        f"vpn={vpn_backend}"
    )

    while True:
        bal = primary_ikas()
        log_line(f"primary {bal:.6f} iKAS")
        if bal >= target_ikas:
            log_line(f"target reached ({bal:.6f} >= {target_ikas})")
            save_state({**state, "next_index": cursor, "primary_ikas": bal, "done": True})
            return 0

        index = next_unclaimed_index(cursor, end)
        if index is None:
            log_line(f"no unclaimed wallets in {cursor}..{end}; extend --end or wait UTC day")
            if once:
                return 1
            do_rotate(
                enabled=auto_rotate,
                backend=vpn_backend,
                rotate_cmd=rotate_cmd,
                locations=locations,
            )
            time.sleep(wait_minutes * 60)
            continue

        do_rotate(
            enabled=auto_rotate,
            backend=vpn_backend,
            rotate_cmd=rotate_cmd,
            locations=locations,
        )

        result = drip_cycle(index)
        if result == "connection_cap":
            log_line(f"connection cap at wallet{index} — rotate and retry")
            if wait_ip_change and last_ip is not None:
                last_ip = wait_for_ip_change(last_ip, poll_seconds)
            else:
                do_rotate(
                    enabled=auto_rotate,
                    backend=vpn_backend,
                    rotate_cmd=rotate_cmd,
                    locations=locations,
                )
                if once:
                    return 2
                time.sleep(max(30.0, wait_minutes * 60))
            continue

        if result == "claimed":
            log_line(f"wallet{index} already claimed — advance")
            cursor = index + 1
            save_state({**state, "next_index": cursor, "primary_ikas": bal})
            if once:
                return 0
            if not auto_rotate and not wait_ip_change:
                time.sleep(wait_minutes * 60)
            continue

        gained = sweep_wallet(index)
        bal = primary_ikas()
        cursor = index + 1
        state = {
            "next_index": cursor,
            "primary_ikas": bal,
            "last_wallet": index,
            "last_gain_ikas": round(gained, 6),
            "updated_at": int(time.time()),
        }
        save_state(state)
        log_line(f"ok wallet{index}  swept ~{gained:.4f} iKAS  primary={bal:.6f} iKAS  next=wallet{cursor}")

        if once:
            return 0
        if wait_ip_change and last_ip is not None:
            last_ip = wait_for_ip_change(last_ip, poll_seconds)
        elif auto_rotate:
            log_line("vpn rotate before next cycle")
            do_rotate(
                enabled=True,
                backend=vpn_backend,
                rotate_cmd=rotate_cmd,
                locations=locations,
            )
        else:
            log_line(f"sleep {wait_minutes}m before next IP window")
            time.sleep(wait_minutes * 60)


def main() -> int:
    parser = argparse.ArgumentParser(description="IP-relay faucet drip + sweep loop")
    parser.add_argument("--start", type=int, default=17, help="first extra wallet index")
    parser.add_argument("--end", type=int, default=400, help="last extra wallet index to try")
    parser.add_argument("--target-ikas", type=float, default=3.2, help="stop when primary reaches this")
    parser.add_argument("--wait-minutes", type=float, default=4.0, help="sleep when not using --auto-rotate")
    parser.add_argument("--once", action="store_true", help="single drip+sweep cycle")
    parser.add_argument(
        "--auto-rotate",
        action="store_true",
        help="call VPN rotate before each drip (Hotspot Shield / Mullvad / custom)",
    )
    parser.add_argument(
        "--wait-ip-change",
        action="store_true",
        help="poll ipify until egress changes, then run next cycle (external IP rotate)",
    )
    parser.add_argument(
        "--poll-seconds",
        type=float,
        default=30.0,
        help="ipify poll interval for --wait-ip-change (default 30)",
    )
    parser.add_argument(
        "--vpn",
        default=os.environ.get("GALLEON_VPN_BACKEND", "hotspot-shield"),
        choices=["hotspot-shield", "mullvad", "nordvpn", "windscribe", "custom"],
        help="VPN backend for --auto-rotate",
    )
    parser.add_argument(
        "--rotate-cmd",
        default=os.environ.get("GALLEON_VPN_ROTATE_CMD"),
        help="custom shell command when --vpn custom",
    )
    parser.add_argument(
        "--locations",
        default=os.environ.get("GALLEON_VPN_LOCATIONS", "us,ca,uk,de,fr,nl"),
        help="comma-separated locations for mullvad/nord rotation",
    )
    args = parser.parse_args()
    if args.auto_rotate and args.vpn == "custom" and not args.rotate_cmd:
        raise SystemExit("--vpn custom requires --rotate-cmd or GALLEON_VPN_ROTATE_CMD")
    return run_relay(
        start=args.start,
        end=args.end,
        target_ikas=args.target_ikas,
        wait_minutes=args.wait_minutes,
        once=args.once,
        auto_rotate=args.auto_rotate,
        wait_ip_change=args.wait_ip_change,
        poll_seconds=args.poll_seconds,
        vpn_backend=args.vpn,
        rotate_cmd=args.rotate_cmd,
        locations=parse_locations(args.locations),
    )


if __name__ == "__main__":
    raise SystemExit(main())
