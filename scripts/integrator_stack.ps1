# Start full TN10 integrator custody stack (background jobs).
# Not consensus. Loopback only.

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$LogDir = Join-Path $Root ".local\stack-logs"
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

function Import-KaspaEnv {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return }
    Get-Content $Path | ForEach-Object {
        if ($_ -match '^\s*([^#=]+)=(.*)$') {
            $name = $matches[1].Trim()
            $value = $matches[2].Trim().Trim('"').Trim("'")
            if ($name) { Set-Item -Path "Env:$name" -Value $value -Force }
        }
    }
}

Import-KaspaEnv (Join-Path $Root "kaspa.env")

$DepositAddr = "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt"
$env:TN10_MIN_HEALTHY = if ($env:TN10_MIN_HEALTHY) { $env:TN10_MIN_HEALTHY } else { "2" }
$env:TN10_DEPOSIT_DATABASE = if ($env:TN10_DEPOSIT_DATABASE) { $env:TN10_DEPOSIT_DATABASE } else { ".local/tn10-wrpc-live.sqlite" }
$env:TN10_WITHDRAWAL_DATABASE = if ($env:TN10_WITHDRAWAL_DATABASE) { $env:TN10_WITHDRAWAL_DATABASE } else { ".local/tn10-withdrawals.sqlite" }
$env:INTEGRATOR_API_BIND = if ($env:INTEGRATOR_API_BIND) { $env:INTEGRATOR_API_BIND } else { "127.0.0.1:8787" }

function Start-StackJob {
    param([string]$Name, [string[]]$CargoArgs)
    $log = Join-Path $LogDir "$Name.log"
    $err = Join-Path $LogDir "$Name.err.log"
    $proc = Start-Process -FilePath "cargo" -ArgumentList $CargoArgs -WorkingDirectory $Root `
        -RedirectStandardOutput $log -RedirectStandardError $err -PassThru -WindowStyle Hidden
    return @{ Name = $Name; Id = $proc.Id; Log = $log }
}

function Stop-StackProcesses {
    @("tn10-wrpc-live", "tn10-integrator-api", "tn10-outbox-receiver") | ForEach-Object {
        Get-Process $_ -ErrorAction SilentlyContinue | Stop-Process -Force
    }
}

if ($args -contains "--stop") {
    Stop-StackProcesses
    Write-Host "Stack processes stopped."
    exit 0
}

if ($args -contains "--status") {
    foreach ($name in @("tn10-wrpc-live", "tn10-integrator-api", "tn10-outbox-receiver")) {
        $p = Get-Process $name -ErrorAction SilentlyContinue
        if ($p) { Write-Host "$name running pid=$($p.Id)" } else { Write-Host "$name not running" }
    }
    curl.exe -s http://127.0.0.1:8787/health 2>$null
    exit 0
}

# Stop stale instances
Stop-StackProcesses

$minHealthy = [string]$env:TN10_MIN_HEALTHY
$depositDb = [string]$env:TN10_DEPOSIT_DATABASE

$jobs = @()
$jobs += Start-StackJob "wrpc-live" @(
    "run", "--release", "--bin", "tn10-wrpc-live", "--",
    $DepositAddr, "--dual", "--min-healthy", $minHealthy,
    "--database", $depositDb
)
Start-Sleep -Seconds 2
$jobs += Start-StackJob "integrator-api" @("run", "--release", "--bin", "tn10-integrator-api")
Start-Sleep -Seconds 1
$jobs += Start-StackJob "outbox-receiver" @(
    "run", "--release", "--bin", "tn10-outbox-receiver", "--",
    "--allow-cleartext-loopback",
    "--database", ".local/tn10-outbox-receiver.sqlite",
    "--dual", "--min-healthy", $minHealthy
)

$pidFile = Join-Path $LogDir "stack.pids.json"
$jobs | ConvertTo-Json | Set-Content $pidFile -Encoding UTF8

Write-Host "=== TN10 integrator stack started ==="
foreach ($j in $jobs) {
    Write-Host "$($j.Name) pid=$($j.Id) log=$($j.Log)"
}
Write-Host "Custody API:  http://127.0.0.1:8787/health"
Write-Host "Demo API:     python examples/integrator_api.py  (port 8788)"
Write-Host "Receiver:     http://127.0.0.1:18320/kaspa-events"
Write-Host "Stop:         powershell -File scripts/integrator_stack.ps1 --stop"
Write-Host "Status:       powershell -File scripts/integrator_stack.ps1 --status"
