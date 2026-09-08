#!/usr/bin/env python3
"""Poll TN10 owned nodes and print IBD stage. Not consensus evidence."""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
from pathlib import Path

import websockets

NODES = (
    ("laptop", "ws://127.0.0.1:18210"),
    ("replica", "ws://127.0.0.1:28210"),
)


def configured_nodes() -> tuple[tuple[str, str], ...]:
    raw = os.environ.get("TN10_OWNED_NODE_URLS", "").strip()
    if not raw:
        return NODES
    urls = [part.strip() for part in raw.split(",") if part.strip()]
    return tuple((f"node{i}", url) for i, url in enumerate(urls, start=1))

DEFAULT_LOG = Path(os.environ.get("LOCALAPPDATA", "")) / "kaspa" / "tn10" / "kaspa-testnet-10" / "logs" / "rusty-kaspa.log"

# Keep aligned with owned_node.rs stage constants.
STAGE_HEALTHY = "healthy"
STAGE_UTXO_COMMIT = "utxo_commit"
STAGE_BODY_SYNC = "body_sync"
STAGE_IBD_PEERS = "ibd_peers"
STAGE_NO_PEERS = "no_peers"
STAGE_FINISHING_SYNC = "finishing_sync"
STAGE_MISSING_UTXOINDEX = "missing_utxoindex"
STAGE_DAG_INCOMPLETE = "dag_incomplete"
MAX_HEADER_BODY_GAP = 100


def peer_list(payload: dict) -> list:
    params = payload.get("params") or {}
    return params.get("peerInfo") or params.get("infos") or []


def count_ibd_peers(peers: list) -> int | None:
    """Return IBD peer count, or None if any peer omits is_ibd_peer (matches wrpc strict parse)."""
    count = 0
    for peer in peers:
        if "is_ibd_peer" in peer:
            if peer["is_ibd_peer"]:
                count += 1
        elif "isIbdPeer" in peer:
            if peer["isIbdPeer"]:
                count += 1
        else:
            return None
    return count


def owned_node_stage(
    *,
    has_utxo_index: bool,
    daa: int,
    blocks: int,
    headers: int,
    ibd_peers: int | None,
    synced: bool,
    connected_peers: int,
) -> str:
    if not has_utxo_index:
        return STAGE_MISSING_UTXOINDEX
    if daa == 0:
        return STAGE_UTXO_COMMIT
    if daa > 0 and headers == 0:
        return STAGE_DAG_INCOMPLETE
    gap = headers - blocks
    if gap > MAX_HEADER_BODY_GAP:
        return STAGE_BODY_SYNC
    if ibd_peers is None:
        return "peer_parse_error"
    if ibd_peers > 0:
        return STAGE_IBD_PEERS
    if connected_peers == 0:
        return STAGE_NO_PEERS
    if not synced:
        return STAGE_FINISHING_SYNC
    return STAGE_HEALTHY


async def wrpc_call(ws, req_id: int, method: str) -> dict:
    await ws.send(json.dumps({"id": req_id, "method": method, "params": {}}))
    return json.loads(await asyncio.wait_for(ws.recv(), timeout=12))


async def probe(url: str) -> dict:
    async with websockets.connect(url, open_timeout=12, close_timeout=2, max_size=8 * 2**20) as ws:
        server = (await wrpc_call(ws, 1, "getServerInfo")).get("params", {})
        dag = (await wrpc_call(ws, 2, "getBlockDagInfo")).get("params", {})
        peers_raw = await wrpc_call(ws, 3, "getConnectedPeerInfo")
        peers = peer_list(peers_raw)
        ibd_peers = count_ibd_peers(peers)
        blocks = int(dag.get("blockCount") or 0)
        headers = int(dag.get("headerCount") or 0)
        daa = int(server.get("virtualDaaScore") or 0)
        return {
            "url": url,
            "synced": server.get("isSynced"),
            "hasUtxoIndex": server.get("hasUtxoIndex"),
            "daa": daa,
            "blocks": blocks,
            "headers": headers,
            "gap": headers - blocks,
            "peer_count": len(peers),
            "ibd_peers": ibd_peers,
            "stage": owned_node_stage(
                has_utxo_index=bool(server.get("hasUtxoIndex")),
                daa=daa,
                blocks=blocks,
                headers=headers,
                ibd_peers=ibd_peers,
                synced=bool(server.get("isSynced")),
                connected_peers=len(peers),
            ),
        }


def utxoindex_resync_hint(log_path: Path) -> str | None:
    if not log_path.is_file():
        return None
    try:
        tail = log_path.read_text(encoding="utf-8", errors="replace")[-400_000:]
    except OSError:
        return None
    marker = "Resyncing the utxoindex..."
    last = tail.rfind(marker)
    if last < 0:
        return None
    after = tail[last + len(marker) :]
    if marker in after:
        return "active (restarted)"
    return "active"


def stage_note(stage: str, utxoindex: str | None) -> str:
    if utxoindex and stage == STAGE_BODY_SYNC:
        return f"utxoindex resync ({utxoindex}); body sync queued"
    return stage


async def collect_nodes(names_urls: tuple[tuple[str, str], ...]) -> list[dict]:
    rows: list[dict] = []
    for name, url in names_urls:
        try:
            data = await probe(url)
        except Exception as error:  # noqa: BLE001 — operator tool
            rows.append({"name": name, "url": url, "error": str(error), "stage": "unreachable"})
            continue
        data["name"] = name
        rows.append(data)
    return rows


async def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Poll TN10 owned-node IBD stages.")
    parser.add_argument(
        "--json",
        action="store_true",
        help="emit machine-readable JSON (supervisor-friendly)",
    )
    args = parser.parse_args(argv)

    log_path = Path(os.environ.get("TN10_KASPAD_LOG", str(DEFAULT_LOG)))
    utxoindex = utxoindex_resync_hint(log_path) if log_path else None
    rows = await collect_nodes(configured_nodes())

    if args.json:
        payload = {"utxoindexResync": utxoindex, "nodes": rows}
        print(json.dumps(payload, indent=2))
        return 0

    if utxoindex:
        print(f"log: {log_path.name} - utxoindex resync {utxoindex}")
        print()

    for row in rows:
        if "error" in row:
            print(f"{row['name']:<8} ERROR  {row['error']}")
            continue
        utxo_hint = utxoindex if row["name"] == "laptop" else None
        ibd_label = "?" if row["ibd_peers"] is None else f"{row['ibd_peers']:>2}"
        print(
            f"{row['name']:<8} DAA {row['daa']:>12} "
            f"bodies/headers {row['blocks']}/{row['headers']} "
            f"gap {row['gap']:>8} peers {row['peer_count']:>2} ibd {ibd_label} "
            f"synced {row['synced']}  "
            f"[{stage_note(row['stage'], utxo_hint)}]"
        )
    print()
    print(
        "gate: cargo run --release --bin tn10-node-health -- --dual --min-healthy 2 --json"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
