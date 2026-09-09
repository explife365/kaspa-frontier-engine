# TN10 dev broadcast - testnet rehearsal only until kaspa-python-sdk PR 78 publishes.
# Applies TN10_SDK_DEV_PATCH=1 (computeBudget to_dict shim). Not production-ready.

param(
    [switch]$SkipGate,
    [string[]]$Apps = @("counter", "timelock_vault", "restricted_swap")
)

if ($Apps.Count -eq 1 -and $Apps[0] -match ",") {
    $Apps = $Apps[0].Split(",", [System.StringSplitOptions]::RemoveEmptyEntries)
}

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$env:TN10_SDK_DEV_PATCH = "1"
$env:TN10_MIN_HEALTHY = "2"
$env:TN10_OWNED_NODE_URLS = "ws://127.0.0.1:18210,ws://127.0.0.1:28210"
$env:PYTHONUNBUFFERED = "1"

Push-Location $root
try {
    if (-not $SkipGate) {
        Write-Host "=== dev SDK gate ==="
        & python "$root\scripts\tn10_sdk_gate.py" --dev --json
        if ($LASTEXITCODE -ne 0) {
            Write-Host "dev patch gate failed - cannot broadcast safely"
            exit 1
        }

        Write-Host "`n=== owned-node health gate ==="
        powershell -File "$root\scripts\tn10_gate.ps1" -Json
        if ($LASTEXITCODE -ne 0) {
            Write-Host "node gate red - fix node 1 + node 2 before broadcast"
            exit 1
        }
    }

    Write-Host "`n=== dev broadcast + fixture publish (TN10 testnet) ==="
    Write-Host "WARNING: dev patch is not a substitute for PR #78 in a published wheel."

    $catalog = @(
        @{ Name = "counter"; Script = "examples\silverscript\counter.py"; Fixture = "fixtures\tn10-counter-proof.json" },
        @{ Name = "timelock_vault"; Script = "examples\silverscript\timelock_vault.py"; Fixture = "fixtures\tn10-vault-proof.json" },
        @{ Name = "restricted_swap"; Script = "examples\silverscript\restricted_swap.py"; Fixture = "fixtures\tn10-swap-proof.json" }
    )
    $selected = @($catalog | Where-Object { $Apps -contains $_.Name })
    if (-not $selected) {
        throw "no apps matched -Apps ($($Apps -join ','))"
    }

    foreach ($app in $selected) {
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

    Write-Host "`nDev broadcast complete. Re-run native gate after published wheel ships."
}
finally {
    Pop-Location
}
