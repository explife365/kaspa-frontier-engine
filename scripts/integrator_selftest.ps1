# End-to-end integrator custody API self-test (loopback).
# Exit 0 when all critical checks pass. Gate may be red if nodes are down.

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

function Import-KaspaEnv {
    $path = Join-Path $Root "kaspa.env"
    if (-not (Test-Path $path)) { throw "kaspa.env missing - run integrator_gen_keys.py" }
    Get-Content $path | ForEach-Object {
        if ($_ -match '^\s*([^#=]+)=(.*)$') {
            $name = $matches[1].Trim()
            $value = $matches[2].Trim().Trim('"').Trim("'")
            if ($name) { Set-Item -Path "Env:$name" -Value $value -Force }
        }
    }
}

function Get-PilotKey {
    $cred = ".local\integrator_pilot_credentials.txt"
    if (-not (Test-Path $cred)) { throw "missing $cred" }
    $line = Get-Content $cred | Where-Object { $_ -match '^pilot_key=' } | Select-Object -First 1
    return ($line -replace '^pilot_key=', '').Trim()
}

Import-KaspaEnv
$key = Get-PilotKey
$headers = @{ "X-Integrator-Key" = $key }
$failures = @()

Write-Host "=== integrator selftest ==="

function Test-Endpoint {
    param([string]$Name, [string]$Uri, [string]$Method = "GET", $Body = $null)
    try {
        if ($Method -eq "GET") {
            $r = Invoke-RestMethod -Uri $Uri -Headers $headers -TimeoutSec 30
        } else {
            $r = Invoke-RestMethod -Uri $Uri -Headers $headers -Method POST -Body ($Body | ConvertTo-Json) -ContentType "application/json" -TimeoutSec 60
        }
        Write-Host "[ok] $Name"
        return $r
    } catch {
        Write-Host "[FAIL] $Name - $($_.Exception.Message)"
        $failures += $Name
        return $null
    }
}

if (-not (Get-Process tn10-integrator-api -ErrorAction SilentlyContinue)) {
    Write-Host "Starting stack..."
    powershell -File scripts/integrator_stack.ps1 | Out-Null
    Start-Sleep -Seconds 15
}

$h = Invoke-RestMethod -Uri "http://127.0.0.1:8787/health" -TimeoutSec 10
if (-not $h.ok) { $failures += "health" }

$self = Test-Endpoint "pilot/selftest" "http://127.0.0.1:8787/v1/pilot/selftest"
$summary = Test-Endpoint "pilot/summary" "http://127.0.0.1:8787/v1/pilot/summary"
Test-Endpoint "deposits" "http://127.0.0.1:8787/v1/deposits?limit=5"
Test-Endpoint "receiver/stats" "http://127.0.0.1:8787/v1/receiver/stats"

try {
    $csv = Invoke-WebRequest -Uri "http://127.0.0.1:8787/v1/deposits/export?format=csv&limit=5" -Headers $headers -TimeoutSec 30
    if ($csv.Content -match "txId") { Write-Host "[ok] deposits/export csv" } else { $failures += "export-csv" }
} catch { $failures += "export-csv" }

$payload = '{"schemaVersion":1,"event":{"id":0,"eventKey":"test:selftest","kind":"credit","txId":"0000000000000000000000000000000000000000000000000000000000000000","outputIndex":0,"amountSompi":1,"address":"kaspatest:qtest"}}'
$sigLine = (python -c "import hmac,hashlib,os; s=os.environ.get('INTEGRATOR_WEBHOOK_SECRET',''); b='$payload'; print('sha256='+hmac.new(s.encode(),b.encode(),hashlib.sha256).hexdigest())")
$verify = Test-Endpoint "webhooks/verify" "http://127.0.0.1:8787/v1/webhooks/verify" "POST" @{ body = $payload; signature = $sigLine }
if ($verify -and -not $verify.ok) { $failures += "webhooks/verify" }

if (Get-Process tn10-outbox-receiver -ErrorAction SilentlyContinue) {
    $wh = Test-Endpoint "webhooks/test" "http://127.0.0.1:8787/v1/webhooks/test" "POST" @{ url = "http://127.0.0.1:18320/kaspa-events" }
    if ($wh -and -not $wh.ok) { Write-Host "[warn] webhooks/test returned non-2xx (receiver may reject test payload)" }
} else {
    Write-Host "[skip] webhooks/test - outbox-receiver not running"
}

if ($self) {
    $gate = $self.checks | Where-Object { $_.name -eq "owned_node_gate" }
    if ($gate -and -not $gate.ok) {
        Write-Host "[warn] gate red - start kaspad before CEX demo"
    }
}

Write-Host ""
if ($failures.Count -eq 0) {
    Write-Host "SELFTEST PASS"
    exit 0
}
Write-Host "SELFTEST FAIL: $($failures -join ', ')"
exit 1
