"""Legacy integrator status API (stdlib HTTP rehearsal).

Production custody API (auth, deposits, outbox, withdrawals):
  cargo run --release --bin tn10-integrator-api
  powershell -File scripts/integrator_go_live.ps1

This Python server remains for dashboard/demo status only:
  python examples/integrator_api.py
  python examples/integrator_api.py --port 8788

Endpoints:
  GET /health
  GET /v1/status?skip_gate=1
  GET /v1/adoption?public_only=1
  GET /v1/fixtures
  GET /v1/blockers
  GET /v1/dex/status
  GET /v1/dex/quote?sell=1.0&buy=wiKAS
  GET /v1/dex/swap?sell=0.5&buy=wiKAS&dry_run=1
  GET /v1/dex/pairs
  GET /v1/cex/readiness?skip_gate=1
  GET /v1/cex/wallets
  GET /v1/cex/validate?skip_gate=1
  GET /v1/onboard?path=nodes

Not production SaaS. No auth. Bind loopback by default.
"""

from __future__ import annotations

import argparse
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlparse

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "examples"))
sys.path.insert(0, str(ROOT / "scripts"))

from integrator_status import FIXTURES, build_report, verify_fixture_offline  # noqa: E402
from kaspa_env import load_kaspa_env  # noqa: E402


def adoption_public_only() -> dict[str, Any]:
    import subprocess

    proc = subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "tn10_adoption_scorecard.py"), "--json", "--public-only"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=60,
        check=False,
    )
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError:
        return {"ok": False, "error": proc.stderr or proc.stdout[:300]}


class IntegratorHandler(BaseHTTPRequestHandler):
    server_version = "integrator-api/0.1"

    def _json(self, code: int, body: dict[str, Any]) -> None:
        payload = json.dumps(body, indent=2).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        qs = parse_qs(parsed.query)
        path = parsed.path.rstrip("/") or "/"

        if path == "/health":
            self._json(200, {"ok": True, "service": "integrator-api", "not_consensus": True})
            return

        if path == "/v1/status":
            skip_gate = qs.get("skip_gate", ["0"])[0] in ("1", "true", "yes")
            report = build_report(skip_gate=skip_gate)
            self._json(200, report)
            return

        if path == "/v1/adoption":
            if qs.get("public_only", ["0"])[0] in ("1", "true", "yes"):
                self._json(200, adoption_public_only())
                return
            skip_gate = qs.get("skip_gate", ["0"])[0] in ("1", "true", "yes")
            from integrator_status import adoption_card

            self._json(200, adoption_card(skip_gate))
            return

        if path == "/v1/blockers":
            from integrator_shims import build_report as shim_report

            self._json(200, shim_report())
            return

        if path == "/v1/dex/status":
            from galleon_dex_common import dex_status

            try:
                self._json(200, dex_status())
            except Exception as err:  # noqa: BLE001
                self._json(503, {"ok": False, "error": str(err)})
            return

        if path == "/v1/dex/quote":
            from galleon_dex_common import dex_quote

            sell = qs.get("sell", [""])[0]
            buy = qs.get("buy", ["wiKAS"])[0]
            if not sell:
                self._json(400, {"ok": False, "error": "sell query param required"})
                return
            try:
                slippage = int(qs.get("slippage_bps", ["50"])[0])
                zero_for_one = buy.lower() in ("wikas", "wiKAS".lower(), "token1", "1")
                body = dex_quote(float(sell), zero_for_one, slippage_bps=slippage)
                body["ok"] = True
                self._json(200, body)
            except Exception as err:  # noqa: BLE001
                self._json(400, {"ok": False, "error": str(err)})
            return

        if path == "/v1/dex/swap":
            from galleon_dex_common import dex_swap_plan

            sell = qs.get("sell", [""])[0]
            buy = qs.get("buy", ["wiKAS"])[0]
            dry_run = qs.get("dry_run", ["1"])[0] in ("1", "true", "yes")
            if not sell:
                self._json(400, {"ok": False, "error": "sell query param required"})
                return
            if not dry_run:
                self._json(
                    403,
                    {
                        "ok": False,
                        "error": "broadcast swaps disabled on integrator API; use dry_run=1 or galleon_dex.py",
                    },
                )
                return
            try:
                slippage = int(qs.get("slippage_bps", ["50"])[0])
                body = dex_swap_plan(float(sell), buy, slippage_bps=slippage)
                body["ok"] = True
                self._json(200, body)
            except Exception as err:  # noqa: BLE001
                self._json(400, {"ok": False, "error": str(err)})
            return

        if path == "/v1/dex/pairs":
            from cex_api_common import dex_pairs

            try:
                self._json(200, dex_pairs())
            except Exception as err:  # noqa: BLE001
                self._json(503, {"ok": False, "error": str(err)})
            return

        if path == "/v1/games/status":
            from galleon_games_common import games_status

            try:
                body = games_status()
                body["ok"] = True
                self._json(200, body)
            except Exception as err:  # noqa: BLE001
                self._json(503, {"ok": False, "error": str(err)})
            return

        if path == "/v1/cex/readiness":
            from cex_api_common import cex_readiness

            skip_gate = qs.get("skip_gate", ["1"])[0] in ("1", "true", "yes")
            body = cex_readiness(skip_gate=skip_gate)
            body["ok"] = True
            self._json(200, body)
            return

        if path == "/v1/cex/wallets":
            from cex_api_common import cex_wallets

            self._json(200, {"ok": True, **cex_wallets()})
            return

        if path == "/v1/cex/validate":
            from cex_api_common import validate_scenarios

            skip_gate = qs.get("skip_gate", ["1"])[0] in ("1", "true", "yes")
            live_dex = qs.get("dex", ["1"])[0] not in ("0", "false", "no")
            self._json(200, validate_scenarios(skip_gate=skip_gate, live_dex=live_dex))
            return

        if path == "/v1/onboard":
            from cex_api_common import onboard_card

            path_id = qs.get("path", ["rest"])[0]
            try:
                self._json(200, onboard_card(path_id))
            except ValueError as err:
                self._json(400, {"ok": False, "error": str(err)})
            return

        if path == "/v1/fixtures":
            items = [verify_fixture_offline(name) for name in FIXTURES]
            self._json(
                200,
                {
                    "all_ok": all(i.get("ok") for i in items),
                    "count": len(items),
                    "items": items,
                },
            )
            return

        self._json(
            404,
            {
                "ok": False,
                "error": "not found",
                "paths": [
                    "/health",
                    "/v1/status",
                    "/v1/adoption",
                    "/v1/fixtures",
                    "/v1/blockers",
                    "/v1/dex/status",
                    "/v1/dex/quote?sell=1.0&buy=wiKAS",
                    "/v1/dex/swap?sell=0.5&buy=wiKAS&dry_run=1",
                    "/v1/dex/pairs",
                    "/v1/games/status",
                    "/v1/cex/readiness",
                    "/v1/cex/wallets",
                    "/v1/cex/validate",
                    "/v1/onboard?path=nodes",
                ],
            },
        )

    def log_message(self, fmt: str, *args: object) -> None:
        sys.stderr.write(f"{self.address_string()} - {fmt % args}\n")


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Integrator status HTTP API")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8788)
    args = parser.parse_args()
    httpd = ThreadingHTTPServer((args.host, args.port), IntegratorHandler)
    print(f"integrator-api listening http://{args.host}:{args.port}")
    print("demo/status only — for CEX custody use: cargo run --release --bin tn10-integrator-api")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nshutdown")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
