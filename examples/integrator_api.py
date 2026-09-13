"""Hosted integrator status API (stdlib HTTP rehearsal).

  python examples/integrator_api.py
  python examples/integrator_api.py --port 8787

Endpoints:
  GET /health
  GET /v1/status?skip_gate=1
  GET /v1/adoption?public_only=1
  GET /v1/fixtures

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

        self._json(404, {"ok": False, "error": "not found", "paths": ["/health", "/v1/status", "/v1/adoption", "/v1/fixtures"]})

    def log_message(self, fmt: str, *args: object) -> None:
        sys.stderr.write(f"{self.address_string()} - {fmt % args}\n")


def main() -> int:
    load_kaspa_env(ROOT)
    parser = argparse.ArgumentParser(description="Integrator status HTTP API")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8787)
    args = parser.parse_args()
    httpd = ThreadingHTTPServer((args.host, args.port), IntegratorHandler)
    print(f"integrator-api listening http://{args.host}:{args.port}")
    print("rehearsal only — not consensus, no auth")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nshutdown")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
