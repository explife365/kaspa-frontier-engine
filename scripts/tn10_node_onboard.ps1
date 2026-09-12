# TN10 owned-node onboarding: start kaspad, watch IBD, run adoption scorecard + gate.
# Not consensus evidence. Fails closed when nodes are unhealthy.
param(
    [switch]$StartNode,
    [switch]$SkipGate,
    [int]$MinHealthy = 2,
    [switch]$Json
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $root ".local\adoption"
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$jsonOut = Join-Path $outDir "adoption_$stamp.json"

$env:TN10_MIN_HEALTHY = "$MinHealthy"
$env:TN10_OWNED_NODE_URLS = "ws://127.0.0.1:18210,ws://127.0.0.1:28210"

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Push-Location $root

function Write-Step($title) {
    Write-Host ""
    Write-Host "=== $title ==="
}

try {
    Write-Step "TN10 public status (REST; works without owned node)"
    cargo run --quiet --release --bin tn10-status 2>&1 | Select-Object -First 25
    python "$root\scripts\tn10_adoption_scorecard.py" --public-only 2>&1 | Select-Object -First 12

    if ($StartNode) {
        Write-Step "Start owned kaspad (node 1)"
        Write-Host "Launching in new window; watch IBD with: python scripts/tn10_ibd_watch.py"
        Start-Process powershell -ArgumentList "-NoExit", "-File", "$root\scripts\tn10_kaspad.ps1"
        Start-Sleep -Seconds 3
    }

    Write-Step "IBD watch"
    python "$root\scripts\tn10_ibd_watch.py"

    Write-Step "Adoption scorecard"
    $scoreArgs = @("$root\scripts\tn10_adoption_scorecard.py", "--min-healthy", "$MinHealthy")
    if ($SkipGate) { $scoreArgs += "--skip-gate" }
    if ($Json) { $scoreArgs += "--json" }

    $scoreText = & python @scoreArgs 2>&1 | Out-String
    Write-Host $scoreText
    if ($Json) {
        $scoreText | Set-Content -Path $jsonOut -Encoding utf8
        Write-Host "wrote $jsonOut"
    }

    if (-not $SkipGate) {
        Write-Step "Integrator rehearsal (optional)"
        Write-Host "When gate is green:"
        Write-Host "  powershell -File scripts/tn10_rehearsal.ps1"
        Write-Host "  powershell -File scripts/tn10_rehearsal.ps1 -Covenant"
        Write-Host "  powershell -File scripts/integrator_evidence_pack.ps1"
    }

    Write-Step "Operator notes"
    Write-Host @"
- Always use --appdir %LOCALAPPDATA%\kaspa\tn10 (see scripts/tn10_kaspad.ps1).
- Do not credit deposits while stage=utxo_commit or gate is red.
- Second node: scripts/tn10_host02_tunnel.ps1 -> ws://127.0.0.1:28210
- Covenant RPC is REST+kascov shim; not kaspad getUtxosByCovenantId.
- Public repo: https://github.com/explife365/kaspa-frontier-engine
"@
}
finally {
    Pop-Location
}
