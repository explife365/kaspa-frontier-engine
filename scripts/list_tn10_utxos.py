#!/usr/bin/env python3
"""List TN10 UTXO sizes for an address."""

from __future__ import annotations

import asyncio
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "examples" / "silverscript"))

from kaspa_env import load_kaspa_env  # noqa: E402
from covenant_common import load_or_create_funder, make_client, utxo_amount  # noqa: E402

load_kaspa_env(ROOT)


async def main() -> None:
    _, addr = load_or_create_funder()
    client = await make_client()
    try:
        result = await client.get_utxos_by_addresses({"addresses": [addr]})
        for entry in sorted(result.get("entries") or [], key=utxo_amount, reverse=True):
            print(f"{utxo_amount(entry):>12} sompi  {entry['outpoint']['transactionId']}:{entry['outpoint']['index']}")
    finally:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
