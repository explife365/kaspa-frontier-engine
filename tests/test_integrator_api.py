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


if __name__ == "__main__":
    unittest.main()
