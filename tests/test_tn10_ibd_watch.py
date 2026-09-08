"""Unit tests for scripts/tn10_ibd_watch.py peer parsing and stage labels."""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "tn10_ibd_watch.py"


def _load_watch():
    spec = importlib.util.spec_from_file_location("tn10_ibd_watch", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules["tn10_ibd_watch"] = module
    spec.loader.exec_module(module)
    return module


def test_count_ibd_peers_requires_explicit_field():
    watch = _load_watch()
    assert watch.count_ibd_peers([{"is_ibd_peer": True}, {"is_ibd_peer": False}]) == 1
    assert watch.count_ibd_peers([{"isIbdPeer": True}]) == 1
    assert watch.count_ibd_peers([{"address": "x"}]) is None
    assert watch.count_ibd_peers([]) == 0


def test_configured_nodes_reads_env(monkeypatch):
    watch = _load_watch()
    monkeypatch.setenv(
        "TN10_OWNED_NODE_URLS",
        "ws://127.0.0.1:18210,ws://127.0.0.1:28210",
    )
    assert watch.configured_nodes() == (
        ("node1", "ws://127.0.0.1:18210"),
        ("node2", "ws://127.0.0.1:28210"),
    )


def test_owned_node_stage_matches_rust_ordering():
    watch = _load_watch()
    base = {
        "has_utxo_index": True,
        "daa": 100,
        "blocks": 100,
        "headers": 100,
        "ibd_peers": 0,
        "synced": True,
        "connected_peers": 3,
    }
    assert watch.owned_node_stage(**base) == watch.STAGE_HEALTHY
    assert watch.owned_node_stage(**{**base, "daa": 0}) == watch.STAGE_UTXO_COMMIT
    assert (
        watch.owned_node_stage(**{**base, "blocks": 1, "headers": 500})
        == watch.STAGE_BODY_SYNC
    )
    assert (
        watch.owned_node_stage(**{**base, "ibd_peers": 2}) == watch.STAGE_IBD_PEERS
    )
