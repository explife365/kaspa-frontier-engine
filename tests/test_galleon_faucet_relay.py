"""Offline tests for galleon_faucet_relay state helpers."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

import galleon_faucet_relay as relay  # noqa: E402


class GalleonFaucetRelayTests(unittest.TestCase):
    def test_parse_locations(self) -> None:
        from galleon_ip_rotate import parse_locations

        self.assertEqual(parse_locations("us, ca; uk"), ["us", "ca", "uk"])

    def test_state_roundtrip(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "state.json"
            with patch.object(relay, "STATE_PATH", path):
                relay.save_state({"next_index": 17, "primary_ikas": 0.27})
                body = relay.load_state()
                self.assertEqual(body["next_index"], 17)
                self.assertAlmostEqual(body["primary_ikas"], 0.27)

    def test_wait_for_ip_change(self) -> None:
        with patch.object(relay, "fetch_egress_ip", return_value="5.6.7.8"):
            with patch.object(relay, "time") as mock_time:
                ip = relay.wait_for_ip_change("1.2.3.4", 30.0)
        self.assertEqual(ip, "5.6.7.8")
        mock_time.sleep.assert_called_once_with(30.0)


if __name__ == "__main__":
    unittest.main()
