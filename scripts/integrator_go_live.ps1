# Start production integrator custody stack (TN10 rehearsal).
# Not consensus. Bind loopback unless fronted by authenticated reverse proxy.

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

function Import-KaspaEnv {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return }
    Get-Content $Path | ForEach-Object {
        if ($_ -match '^\s*([^#=]+)=(.*)$') {
            $name = $matches[1].Trim()
            $value = $matches[2].Trim().Trim('"').Trim("'")
            if ($name -and -not [Environment]::GetEnvironmentVariable($name)) {
                Set-Item -Path "Env:$name" -Value $value
            }
        }
    }
}

Import-KaspaEnv (Join-Path $Root "kaspa.env")

if (-not (Test-Path "kaspa.env")) {
    Write-Warning "kaspa.env missing — run: python scripts/integrator_gen_keys.py"
} elseif (-not $env:INTEGRATOR_API_KEYS) {
    Write-Warning "INTEGRATOR_API_KEYS unset — run: python scripts/integrator_gen_keys.py"
}

$env:TN10_MIN_HEALTHY = if ($env:TN10_MIN_HEALTHY) { $env:TN10_MIN_HEALTHY } else { "2" }
$env:TN10_OWNED_NODE_URLS = if ($env:TN10_OWNED_NODE_URLS) { $env:TN10_OWNED_NODE_URLS } else { "ws://127.0.0.1:18210,ws://127.0.0.1:28210" }
$env:INTEGRATOR_API_BIND = if ($env:INTEGRATOR_API_BIND) { $env:INTEGRATOR_API_BIND } else { "127.0.0.1:8787" }
$env:TN10_DEPOSIT_DATABASE = if ($env:TN10_DEPOSIT_DATABASE) { $env:TN10_DEPOSIT_DATABASE } else { ".local/tn10-wrpc-live.sqlite" }
$env:TN10_WITHDRAWAL_DATABASE = if ($env:TN10_WITHDRAWAL_DATABASE) { $env:TN10_WITHDRAWAL_DATABASE } else { ".local/tn10-withdrawals.sqlite" }

Write-Host "=== TN10 integrator go-live stack ==="
Write-Host "gate:      cargo run --release --bin tn10-node-health -- --dual --min-healthy $env:TN10_MIN_HEALTHY"
Write-Host "ingest:    cargo run --release --bin tn10-wrpc-live -- <addrs> --dual --database $env:TN10_DEPOSIT_DATABASE"
Write-Host "api:       cargo run --release --bin tn10-integrator-api"
Write-Host "receiver:  cargo run --release --bin tn10-outbox-receiver"
Write-Host "openapi:   http://$env:INTEGRATOR_API_BIND/openapi.json"
Write-Host ""

if ($args -contains "--api-only") {
    cargo run --release --bin tn10-integrator-api
    exit $LASTEXITCODE
}

if ($args -contains "--check") {
    cargo run --release --bin tn10-node-health -- --dual --min-healthy $env:TN10_MIN_HEALTHY --json
    exit $LASTEXITCODE
}

Write-Host "Starting integrator API only. Run wrpc-live and outbox-receiver in separate terminals."
cargo run --release --bin tn10-integrator-api
