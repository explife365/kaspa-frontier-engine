"""Offline tests for integrator HTTP API handler."""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))

from integrator_api import IntegratorHandler  # noqa: E402


class _FakeWriter:
    def __init__(self) -> None:
        self.headers: list[tuple[str, str]] = []
        self.status: int | None = None
        self.body = b""

    def write(self, data: bytes) -> None:
        self.body += data


class IntegratorApiTests(unittest.TestCase):
    def _get(self, path: str) -> dict:
        handler = IntegratorHandler.__new__(IntegratorHandler)
        handler.headers = {}
        writer = _FakeWriter()
        handler.wfile = writer
        handler.requestline = f"GET {path} HTTP/1.1"
        handler.request_version = "HTTP/1.1"
        handler.command = "GET"
        handler.path = path
        handler.send_response = lambda code: setattr(writer, "status", code)
        handler.send_header = lambda k, v: writer.headers.append((k, v))
        handler.end_headers = lambda: None
        handler.do_GET()
        return json.loads(writer.body.decode("utf-8"))

    def test_health(self) -> None:
        body = self._get("/health")
        self.assertTrue(body.get("ok"))
        self.assertTrue(body.get("not_consensus"))

    def test_fixtures_endpoint(self) -> None:
        body = self._get("/v1/fixtures")
        self.assertIn("items", body)
        self.assertEqual(body.get("count"), 8)

    def test_cex_wallets(self) -> None:
        body = self._get("/v1/cex/wallets")
        self.assertTrue(body.get("ok"))
        self.assertGreaterEqual(body.get("count", 0), 1)

    def test_cex_readiness(self) -> None:
        body = self._get("/v1/cex/readiness?skip_gate=1")
        self.assertTrue(body.get("ok"))
        self.assertIn("checks", body)

    def test_onboard_nodes(self) -> None:
        body = self._get("/v1/onboard?path=nodes")
        self.assertTrue(body.get("ok"))
        self.assertEqual(body.get("id"), "nodes")

    def test_cex_validate_offline_dex(self) -> None:
        body = self._get("/v1/cex/validate?skip_gate=1&dex=0")
        self.assertIn("scenarios", body)
        self.assertGreaterEqual(body.get("total", 0), 4)

    def test_dex_swap_dry_run_rejects_broadcast(self) -> None:
        body = self._get("/v1/dex/swap?sell=0.5&buy=wiKAS&dry_run=0")
        self.assertFalse(body.get("ok"))
        self.assertEqual(body.get("error", "").lower().find("broadcast"), 0)


if __name__ == "__main__":
    unittest.main()
