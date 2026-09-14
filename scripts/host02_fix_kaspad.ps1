# Free host02 disk for kaspad-tn10, restart service, verify loopback wRPC.
# Root cause when broken: /var/lib/tuce_ops/mamajama_runs fills disk; kaspad panics ENOSPC.
# Mamajama stays OFF for TN10 adoption (see scripts/kip2_sat_host02.md).
param(
    [switch]$PruneMamajama = $true,
    [switch]$SkipRestart
)

$ErrorActionPreference = "Stop"
$key = Join-Path $env:USERPROFILE ".ssh\tuce_mgmt"
$host = "root@host02.tuce.app"
if (-not (Test-Path $key)) {
    throw "missing SSH key $key"
}

$prune = if ($PruneMamajama) { "1" } else { "0" }
$skip = if ($SkipRestart) { "1" } else { "0" }
$cmd = @"
set -euo pipefail
echo '=== disk before ==='
df -h / | tail -1
if [ '$prune' = '1' ] && [ -d /var/lib/tuce_ops/mamajama_runs ]; then
  echo 'pruning /var/lib/tuce_ops/mamajama_runs (mamajama off for TN10 path)'
  rm -rf /var/lib/tuce_ops/mamajama_runs
  mkdir -p /var/lib/tuce_ops/mamajama_runs
fi
rm -f /tmp/tuce_cert.snapshot.*.db 2>/dev/null || true
journalctl --vacuum-size=200M 2>/dev/null || true
echo '=== disk after prune ==='
df -h / | tail -1
if [ '$skip' != '1' ]; then
  systemctl restart kaspad-tn10
  sleep 8
  systemctl is-active kaspad-tn10
  ss -tlnp | grep 18210 || echo '18210 not listening yet'
fi
"@

Write-Host "=== host02 disk prune + kaspad-tn10 restart ==="
ssh -i $key -o BatchMode=yes -o IdentitiesOnly=yes $host $cmd

Write-Host ""
Write-Host "Start tunnel (separate window): powershell -File scripts/tn10_host02_tunnel.ps1"
Write-Host "Then: python scripts/tn10_adoption_scorecard.py"
