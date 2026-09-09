#!/usr/bin/env python3
"""Print TN10 funding wallet balance for covenant apps."""

from __future__ import annotations

import asyncio
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "examples" / "silverscript"))

from kaspa_env import load_kaspa_env  # noqa: E402
from covenant_common import (  # noqa: E402
    MIN_FUNDING_SOMPI,
    MIN_GENESIS_UTXO_SOMPI,
    load_or_create_funder,
    make_client,
    utxo_amount,
)

load_kaspa_env(ROOT)


async def main() -> int:
    _, addr = load_or_create_funder()
    client = await make_client()
    try:
        result = await client.get_utxos_by_addresses({"addresses": [addr]})
        entries = result.get("entries") or []
        total = sum(utxo_amount(e) for e in entries)
        funded = [e for e in entries if utxo_amount(e) >= MIN_GENESIS_UTXO_SOMPI]
        ready = bool(funded) and (
            max(utxo_amount(e) for e in funded) >= MIN_FUNDING_SOMPI
            or sum(utxo_amount(e) for e in funded) >= MIN_FUNDING_SOMPI
        )
        print(f"address {addr}")
        print(f"utxos {len(entries)} total_sompi {total} spendable_utxos {len(funded)} ready {ready}")
        return 0 if ready else 1
    finally:
        await client.disconnect()


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
