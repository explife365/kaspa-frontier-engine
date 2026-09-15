"""Live-loopback tests for tn10-integrator-api (optional; skips if API down)."""

from __future__ import annotations

import json
import os
import sys
import unittest
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "http://127.0.0.1:8787"


def _pilot_key() -> str | None:
    cred = ROOT / ".local" / "integrator_pilot_credentials.txt"
    if not cred.is_file():
        return os.environ.get("INTEGRATOR_API_KEY")
    for line in cred.read_text(encoding="utf-8").splitlines():
        if line.startswith("pilot_key="):
            return line.partition("=")[2].strip()
    return None


def _get(path: str, key: str) -> dict:
    req = urllib.request.Request(
        f"{BASE}{path}",
        headers={"X-Integrator-Key": key},
    )
    with urllib.request.urlopen(req, timeout=15) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _api_up() -> bool:
    try:
        with urllib.request.urlopen(f"{BASE}/health", timeout=3) as resp:
            body = json.loads(resp.read().decode("utf-8"))
            return bool(body.get("ok"))
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError):
        return False


@unittest.skipUnless(_api_up(), "tn10-integrator-api not running on :8787")
class IntegratorCustodyApiLiveTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.key = _pilot_key()
        if not cls.key:
            raise unittest.SkipTest("pilot key missing")

    def test_health_public(self) -> None:
        with urllib.request.urlopen(f"{BASE}/health", timeout=5) as resp:
            body = json.loads(resp.read().decode("utf-8"))
        self.assertTrue(body.get("ok"))

    def test_pilot_summary(self) -> None:
        body = _get("/v1/pilot/summary", self.key)
        self.assertTrue(body.get("ok"))
        self.assertIn("deposits", body)
        self.assertIn("outbox", body)

    def test_pilot_selftest(self) -> None:
        body = _get("/v1/pilot/selftest", self.key)
        self.assertIn("checks", body)
        names = {c["name"] for c in body["checks"]}
        self.assertIn("deposit_ledger", names)
        self.assertIn("webhook_secret", names)

    def test_deposits_export_csv(self) -> None:
        req = urllib.request.Request(
            f"{BASE}/v1/deposits/export?format=csv&limit=5",
            headers={"X-Integrator-Key": self.key},
        )
        with urllib.request.urlopen(req, timeout=15) as resp:
            text = resp.read().decode("utf-8")
        self.assertIn("txId", text)

    def test_receiver_stats(self) -> None:
        body = _get("/v1/receiver/stats", self.key)
        self.assertIn("count", body)


if __name__ == "__main__":
    unittest.main()
