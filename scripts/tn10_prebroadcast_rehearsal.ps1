# TN10 pre-broadcast rehearsal — everything except on-chain submit.
# Runs while waiting for kaspa-python-sdk#78 published wheel.
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
    powershell -File "$root\scripts\tn10_covenant_rehearsal.ps1"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host "`n=== owned-node health gate ==="
    powershell -File "$root\scripts\tn10_gate.ps1" -Json
    $gateExit = $LASTEXITCODE
    if ($gateExit -ne 0) {
        Write-Warning "node gate not green — broadcast would fail-closed until 2/2 healthy"
    }

    Write-Host "`n=== summary ==="
    Write-Host "native SDK ready: $(if ($nativeExit -eq 0) { 'yes' } else { 'no (expected until PR #78 wheel)' })"
    Write-Host "dev patch ready:  $(if ($devExit -eq 0) { 'yes — TN10 dev broadcast available' } else { 'no' })"
    Write-Host "node gate:        $(if ($gateExit -eq 0) { 'green' } else { 'red' })"

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
