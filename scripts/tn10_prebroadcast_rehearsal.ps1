# TN10 pre-broadcast rehearsal - everything except on-chain submit.
# Runs while waiting for kaspa-python-sdk PR 78 published wheel.
# Not consensus evidence.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$env:TN10_MIN_HEALTHY = "2"
$env:TN10_OWNED_NODE_URLS = "ws://127.0.0.1:18210,ws://127.0.0.1:28210"

Push-Location $root
try {
    Write-Host "=== SDK gate (native) ==="
    & python "$root\scripts\tn10_sdk_gate.py" --json
    $nativeExit = $LASTEXITCODE

    Write-Host "`n=== SDK gate (dev patch rehearsal) ==="
    $env:TN10_SDK_DEV_PATCH = "1"
    & python "$root\scripts\tn10_sdk_gate.py" --dev --json
    $devExit = $LASTEXITCODE
    Remove-Item Env:TN10_SDK_DEV_PATCH -ErrorAction SilentlyContinue

    Write-Host "`n=== SilverScript compile + covenant rehearsal ==="
    $env:TN10_SDK_DEV_PATCH = "1"
    powershell -File "$root\scripts\tn10_covenant_rehearsal.ps1"
    $covenantExit = $LASTEXITCODE
    Remove-Item Env:TN10_SDK_DEV_PATCH -ErrorAction SilentlyContinue
    if ($covenantExit -ne 0) { exit $covenantExit }

    Write-Host "`n=== owned-node health gate ==="
    powershell -File "$root\scripts\tn10_gate.ps1" -Json
    $gateExit = $LASTEXITCODE
    if ($gateExit -ne 0) {
        Write-Warning "node gate not green - broadcast would fail-closed until 2/2 healthy"
    }

    Write-Host "`n=== summary ==="
    if ($nativeExit -eq 0) {
        Write-Host "native SDK ready: yes"
    } else {
        Write-Host "native SDK ready: no (expected until PR 78 wheel)"
    }
    if ($devExit -eq 0) {
        Write-Host "dev patch ready: yes (TN10 dev broadcast available)"
    } else {
        Write-Host "dev patch ready: no"
    }
    if ($gateExit -eq 0) {
        Write-Host "node gate: green"
    } else {
        Write-Host "node gate: red"
    }

    if ($devExit -eq 0 -and $gateExit -eq 0) {
        Write-Host "`nOptional TN10 dev broadcast (testnet only):"
        Write-Host "  powershell -File scripts/tn10_dev_broadcast.ps1"
    }
    if ($nativeExit -eq 0) {
        Write-Host "`nPublished wheel path:"
        Write-Host "  powershell -File scripts/tn10_fixture_publish_when_ready.ps1"
    }
}
finally {
    Pop-Location
}
