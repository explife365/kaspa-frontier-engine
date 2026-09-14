"""Rotate egress IP for Igra faucet relay (VPN disconnect/reconnect).

Windows Hotspot Shield has no official location CLI; we cycle by killing hydra/hsscp
and reconnecting (often yields a new IP on auto-location). Mullvad/Nord/Windscribe
CLIs are used when installed.

  python scripts/galleon_ip_rotate.py --backend hotspot-shield
  python scripts/galleon_ip_rotate.py --backend mullvad --location us
  python scripts/galleon_ip_rotate.py --cmd "my-custom-rotate.bat"
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
IPIFY = "https://api.ipify.org?format=json"
STATE_PATH = ROOT / ".local" / "galleon-ip-rotate.json"


def log(msg: str) -> None:
    print(msg, flush=True)


def fetch_egress_ip() -> str:
    req = urllib.request.Request(IPIFY, headers={"User-Agent": "kaspa-frontier-engine/relay"})
    with urllib.request.urlopen(req, timeout=20) as resp:
        body = json.loads(resp.read().decode("utf-8"))
    ip = str(body.get("ip", "")).strip()
    if not ip:
        raise RuntimeError("ipify returned no ip")
    return ip


def load_rotate_state() -> dict:
    if not STATE_PATH.is_file():
        return {"location_index": 0}
    return json.loads(STATE_PATH.read_text(encoding="utf-8"))


def save_rotate_state(body: dict) -> None:
    STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
    STATE_PATH.write_text(json.dumps(body, indent=2), encoding="utf-8")


def next_location(locations: list[str]) -> str | None:
    if not locations:
        return None
    state = load_rotate_state()
    idx = int(state.get("location_index", 0)) % len(locations)
    loc = locations[idx]
    save_rotate_state({**state, "location_index": idx + 1})
    return loc


def parse_locations(raw: str | None) -> list[str]:
    if not raw:
        raw = os.environ.get("GALLEON_VPN_LOCATIONS", "")
    return [part.strip() for part in raw.replace(";", ",").split(",") if part.strip()]


def run_powershell(backend: str, location: str | None) -> None:
    script = ROOT / "scripts" / "galleon_ip_rotate.ps1"
    if not script.is_file():
        raise RuntimeError(f"missing {script}")
    cmd = [
        "powershell",
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        str(script),
        "-Backend",
        backend,
    ]
    if location:
        cmd.extend(["-Location", location])
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.stdout:
        print(proc.stdout.strip())
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr.strip() or proc.stdout.strip() or "vpn rotate failed")


def run_linux_hotspotshield(location: str | None) -> None:
    loc = location or "US"
    for cmd in (
        ["hotspotshield", "disconnect"],
        ["hotspotshield", "connect", loc],
    ):
        proc = subprocess.run(cmd, capture_output=True, text=True)
        if proc.returncode != 0:
            raise RuntimeError(proc.stderr.strip() or proc.stdout.strip() or f"failed: {cmd}")
        if proc.stdout.strip():
            print(proc.stdout.strip())


def rotate(
    *,
    backend: str,
    custom_cmd: str | None = None,
    location: str | None = None,
    locations: list[str] | None = None,
) -> str:
    """Rotate VPN; return new egress IP (best effort)."""
    before = fetch_egress_ip()
    log(f"egress before  {before}")

    if custom_cmd:
        proc = subprocess.run(custom_cmd, shell=True)
        if proc.returncode != 0:
            raise RuntimeError(f"custom rotate failed: {custom_cmd}")
    elif backend == "hotspot-shield" and sys.platform != "win32":
        run_linux_hotspotshield(location or next_location(locations or parse_locations(None)))
    elif sys.platform == "win32":
        loc = location or next_location(locations or parse_locations(None))
        run_powershell(backend, loc)
    else:
        raise RuntimeError(f"unsupported backend {backend} on {sys.platform}")

    deadline = time.time() + 120
    after = before
    while time.time() < deadline:
        time.sleep(5)
        try:
            after = fetch_egress_ip()
        except (urllib.error.URLError, TimeoutError, RuntimeError):
            continue
        if after != before:
            log(f"egress after   {after}")
            return after
    log(f"egress unchanged ({after}); continuing anyway")
    return after


def main() -> int:
    parser = argparse.ArgumentParser(description="Rotate VPN egress for faucet relay")
    parser.add_argument(
        "--backend",
        default=os.environ.get("GALLEON_VPN_BACKEND", "hotspot-shield"),
        choices=["hotspot-shield", "mullvad", "nordvpn", "windscribe", "custom"],
    )
    parser.add_argument("--location", help="optional location/country code")
    parser.add_argument("--locations", help="comma-separated rotation list (mullvad/nord/linux HSS)")
    parser.add_argument("--cmd", help="custom shell command (backend=custom)")
    args = parser.parse_args()
    locs = parse_locations(args.locations)
    rotate(
        backend=args.backend,
        custom_cmd=args.cmd or os.environ.get("GALLEON_VPN_ROTATE_CMD"),
        location=args.location,
        locations=locs,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
