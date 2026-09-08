# Publish TN10 covenant proof fixtures when the SDK gate turns green.
# Fail-closed until kaspa-python-sdk#78 is in a published wheel with SilverScript.
# Not consensus evidence.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root

$gateJson = & python "$root\scripts\tn10_sdk_gate.py" --json 2>&1 | Out-String
Write-Host $gateJson
$gate = $gateJson | ConvertFrom-Json

if (-not $gate.ready) {
    Write-Host "SDK gate not ready (computeBudget + SilverScript required). Blocked on kaspa-python-sdk#78."
    Write-Host "PR: $($gate.pr.url)"
    exit 1
}

Write-Host "SDK gate green — broadcasting reference covenant apps and publishing fixtures..."

$apps = @(
    @{
        Name = "counter"
        Script = "examples\silverscript\counter.py"
        Fixture = "fixtures\tn10-counter-proof.json"
        LocalProof = ".local\tn10-covenant-proof.json"
        PublishFlag = $false
    },
    @{
        Name = "timelock_vault"
        Script = "examples\silverscript\timelock_vault.py"
        Fixture = "fixtures\tn10-vault-proof.json"
        PublishFlag = $true
    },
    @{
        Name = "restricted_swap"
        Script = "examples\silverscript\restricted_swap.py"
        Fixture = "fixtures\tn10-swap-proof.json"
        PublishFlag = $true
    }
)

foreach ($app in $apps) {
    Write-Host "`n=== $($app.Name) ==="
    if ($app.PublishFlag) {
        & python "$root\$($app.Script)" --publish-fixture
        if ($LASTEXITCODE -ne 0) {
            throw "$($app.Name) broadcast/publish failed with exit $LASTEXITCODE"
        }
    } else {
        & python "$root\$($app.Script)"
        if ($LASTEXITCODE -ne 0) {
            throw "$($app.Name) broadcast failed with exit $LASTEXITCODE"
        }
        $local = Join-Path $root $app.LocalProof
        $fixture = Join-Path $root $app.Fixture
        if (-not (Test-Path $local)) {
            throw "missing $($app.LocalProof) after counter run"
        }
        Copy-Item -Force $local $fixture
        Write-Host "published $fixture"
    }
    & cargo run --quiet --release --bin tn10-proof -- $app.Fixture --json
    if ($LASTEXITCODE -ne 0) {
        throw "tn10-proof failed for $($app.Fixture)"
    }
}

Write-Host "`nAll fixtures published and verified."
exit 0
