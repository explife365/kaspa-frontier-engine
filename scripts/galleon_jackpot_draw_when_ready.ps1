# Wait for jackpot round end, then broadcast draw. Galleon testnet only.

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$status = python examples/galleon_jackpot.py --status 2>&1 | Out-String
Write-Host $status
if ($status -match 'ends\s+(\d+)\s+\((\d+)s left\)') {
    $ends = [int]$matches[1]
    $left = [int]$matches[2]
    if ($left -gt 0) {
        Write-Host "Waiting $left seconds for round end..."
        Start-Sleep -Seconds ($left + 5)
    }
}

python examples/galleon_jackpot.py --draw --broadcast
python examples/galleon_jackpot.py --status
