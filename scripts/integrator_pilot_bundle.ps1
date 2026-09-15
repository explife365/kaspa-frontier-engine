# CEX pilot week-1 handoff bundle → .local/pilot_handoff/
# Requires custody API running for live summary (optional).

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$Stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$Out = Join-Path $Root ".local\pilot_handoff\$Stamp"
New-Item -ItemType Directory -Force -Path $Out | Out-Null

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

Copy-Item "docs\integrator_openapi.yaml" $Out
Copy-Item "docs\integrator_go_live.md" $Out
Copy-Item "scripts\integrator_exchange_pitch.md" $Out
Copy-Item "scripts\cex_production_runbook.md" $Out

$evidence = Get-ChildItem ".local\evidence\evidence_*.json" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($evidence) { Copy-Item $evidence.FullName $Out }

$key = $null
$credPath = ".local\integrator_pilot_credentials.txt"
if (Test-Path $credPath) {
    $key = (Get-Content $credPath | Where-Object { $_ -match '^pilot_key=' }) -replace 'pilot_key=', ''
}

$summaryPath = Join-Path $Out "pilot_summary.json"
if ($key) {
    try {
        $headers = @{ "X-Integrator-Key" = $key }
        Invoke-RestMethod -Uri "http://127.0.0.1:8787/v1/pilot/summary" -Headers $headers |
            ConvertTo-Json -Depth 12 | Set-Content $summaryPath -Encoding UTF8
        Write-Host "Live pilot summary: $summaryPath"
    } catch {
        Write-Warning "Custody API offline — summary skipped ($($_.Exception.Message))"
    }
}

$readme = @"
# TN10 pilot handoff ($Stamp)

Not consensus. Testnet-10 rehearsal only.

## Attach to CEX reply
- integrator_exchange_pitch.md
- integrator_openapi.yaml
- evidence_*.json (if present)
- pilot_summary.json (if API was up)
- Video: Kaspa Video\out\kaspa_dev_quickstart.mp4

## Staging endpoints (your mTLS proxy in production)
- GET /v1/pilot/summary
- GET /v1/gate
- GET /v1/deposits
- POST /v1/watchlist
- POST /v1/webhooks/test

Repo: https://github.com/explife365/kaspa-frontier-engine
"@
Set-Content (Join-Path $Out "README.txt") $readme -Encoding UTF8

Write-Host "Pilot bundle: $Out"
