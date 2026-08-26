$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Push-Location $root
try {
    if (-not (Test-Path "Cargo.lock" -PathType Leaf)) {
        throw "Cargo.lock is required for the offline smoke gate"
    }

    $rustVersion = (& rustc --version)
    if ($LASTEXITCODE -ne 0 -or $rustVersion -notmatch '^rustc 1\.80\.') {
        throw "Rust 1.80.x is required; found: $rustVersion"
    }

    & cargo test --locked --offline --all-targets
    if ($LASTEXITCODE -ne 0) {
        throw "locked offline Rust tests failed"
    }

    & python -m unittest discover -s tests -p "test_*.py"
    if ($LASTEXITCODE -ne 0) {
        throw "offline Python tests failed"
    }

    Write-Host "offline smoke gate passed ($rustVersion)"
}
finally {
    Pop-Location
}
