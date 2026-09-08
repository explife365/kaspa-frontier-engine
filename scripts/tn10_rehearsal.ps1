# Quick TN10 integrator rehearsal: IBD watch, N-of-M gate, dual wrpc resnapshot.
# Not consensus evidence. Requires two loopback TN10 nodes (18210 + 28210).
param(
    [switch]$Covenant
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$address = "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt"

$env:TN10_MIN_HEALTHY = "2"
$env:TN10_OWNED_NODE_URLS = "ws://127.0.0.1:18210,ws://127.0.0.1:28210"

Push-Location $root
try {
    Write-Host "=== TN10 IBD watch ==="
    python "$root\scripts\tn10_ibd_watch.py"

    Write-Host "`n=== N-of-M health gate ==="
    powershell -File "$root\scripts\tn10_gate.ps1" -Json
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "health gate red - fix nodes/tunnel before live ingestion"
        exit $LASTEXITCODE
    }

    Write-Host "`n=== dual wrpc resnapshot ==="
    cargo run --release --bin tn10-wrpc-live -- $address --dual --resnapshot-only

    if ($Covenant) {
        Write-Host "`n=== L1 covenant integrator rehearsal ==="
        powershell -File "$root\scripts\tn10_covenant_rehearsal.ps1"
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
}
finally {
    Pop-Location
}
