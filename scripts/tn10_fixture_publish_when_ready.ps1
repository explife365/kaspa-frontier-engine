# Publish TN10 covenant proof fixtures when the SDK gate turns green.
# Fail-closed until kaspa-python-sdk PR 78 is in a published wheel with SilverScript.
# Not consensus evidence.

param(
    [switch]$Dev
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root

if ($Dev) {
    $env:TN10_SDK_DEV_PATCH = "1"
    $gateJson = & python "$root\scripts\tn10_sdk_gate.py" --dev --json 2>&1 | Out-String
} else {
    $gateJson = & python "$root\scripts\tn10_sdk_gate.py" --json 2>&1 | Out-String
}
Write-Host $gateJson
$gate = $gateJson | ConvertFrom-Json

if (-not $gate.ready) {
    if ($Dev -and $gate.readyWithDevPatch) {
        Write-Host "Using dev patch gate (TN10 testnet rehearsal only)."
    } elseif (-not $Dev) {
        Write-Host "SDK gate not ready (computeBudget + SilverScript required). Blocked on kaspa-python-sdk PR 78."
        Write-Host "PR: $($gate.pr78Url)"
        exit 1
    } else {
        Write-Host "Dev patch gate not ready."
        exit 1
    }
}

Write-Host "SDK gate green - broadcasting reference covenant apps and publishing fixtures..."

$apps = @(
    @{
        Name = "counter"
        Script = "examples\silverscript\counter.py"
        Fixture = "fixtures\tn10-counter-proof.json"
    },
    @{
        Name = "timelock_vault"
        Script = "examples\silverscript\timelock_vault.py"
        Fixture = "fixtures\tn10-vault-proof.json"
    },
    @{
        Name = "restricted_swap"
        Script = "examples\silverscript\restricted_swap.py"
        Fixture = "fixtures\tn10-swap-proof.json"
    }
)

foreach ($app in $apps) {
    Write-Host "`n=== $($app.Name) ==="
    & python "$root\$($app.Script)" --publish-fixture
    if ($LASTEXITCODE -ne 0) {
        throw "$($app.Name) broadcast/publish failed with exit $LASTEXITCODE"
    }
    & cargo run --quiet --release --bin tn10-proof -- $app.Fixture --capture-fixtures
    if ($LASTEXITCODE -ne 0) {
        throw "fixture capture failed for $($app.Fixture)"
    }
    & cargo run --quiet --release --bin tn10-proof -- $app.Fixture --offline --json
    if ($LASTEXITCODE -ne 0) {
        throw "tn10-proof offline failed for $($app.Fixture)"
    }
    & cargo run --quiet --release --bin tn10-proof -- $app.Fixture --kascov-only --json
    if ($LASTEXITCODE -ne 0) {
        throw "tn10-proof kascov verify failed for $($app.Fixture)"
    }
}

Write-Host "`nAll fixtures published, captured, and verified."
exit 0
