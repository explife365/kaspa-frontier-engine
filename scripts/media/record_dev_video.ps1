# Prep terminal + browser for dev video recording (scripts/media/dev_video_script.txt)
# Does not record — opens dashboard and prints commands in order.

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $Root

function Import-KaspaEnv {
    $path = Join-Path $Root "kaspa.env"
    if (-not (Test-Path $path)) { return }
    Get-Content $path | ForEach-Object {
        if ($_ -match '^\s*([^#=]+)=(.*)$') {
            $name = $matches[1].Trim()
            $value = $matches[2].Trim().Trim('"').Trim("'")
            if ($name) { Set-Item -Path "Env:$name" -Value $value -Force }
        }
    }
}

Import-KaspaEnv

Write-Host "=== Dev video prep ==="
Write-Host "Script: scripts/media/dev_video_script.txt"
Write-Host ""

$stack = powershell -File scripts/integrator_stack.ps1 --status 2>&1 | Out-String
if ($stack -notmatch "tn10-integrator-api running") {
    Write-Host "Starting custody stack..."
    powershell -File scripts/integrator_stack.ps1 | Out-Null
    Start-Sleep -Seconds 8
}

$demo = Get-Process python -ErrorAction SilentlyContinue
if (-not $demo) {
    Write-Host "Start demo API in another terminal:"
    Write-Host "  python examples/integrator_api.py"
}

$dashboard = Join-Path $Root "examples\kaspa_frontier_dashboard.html"
if (Test-Path $dashboard) {
    Start-Process $dashboard
}

Write-Host ""
Write-Host "Record these in order:"
Write-Host "  1. python examples/dev_quickstart.py --path rest"
Write-Host "  2. python examples/galleon_games.py coin-flip --heads --bet 1.0 --dry-run"
Write-Host "  3. python scripts/integrator_shims.py --json"
Write-Host "  4. curl http://127.0.0.1:8788/v1/cex/readiness?skip_gate=1"
Write-Host "  5. curl http://127.0.0.1:8787/health"
Write-Host "  6. powershell -File scripts/tn10_node_onboard.ps1  (optional clip)"
Write-Host ""
Write-Host "Dashboard: examples/kaspa_frontier_dashboard.html"
Write-Host "Games UI:  examples/galleon_games.html"
