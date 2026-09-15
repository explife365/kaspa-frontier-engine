# Audit communication channels + infra readiness (no inbox access).
# Run daily: powershell -File scripts/cex_outreach/check_channels.ps1

$ErrorActionPreference = "Continue"
$Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $Root

$OutreachSent = [datetime]"2026-09-14"
$FollowUpStart = [datetime]"2026-09-19"
$FollowUpEnd = [datetime]"2026-09-21"
$Today = Get-Date
$VideoPath = "C:\Users\Admin2\IONOS HiDrive\Build Files\Kaspa Video\out\kaspa_dev_quickstart.mp4"

function Test-FileFresh {
    param([string]$Path, [int]$MaxHours = 48)
    if (-not (Test-Path $Path)) { return "missing" }
    $age = (Get-Date) - (Get-Item $Path).LastWriteTime
    if ($age.TotalHours -gt $MaxHours) { return "stale ($([int]$age.TotalHours)h)" }
    return "ok ($([int]$age.TotalHours)h)"
}

Write-Host "=== Communication channels - $($Today.ToString('yyyy-MM-dd HH:mm')) ==="
Write-Host ""

Write-Host "## CEX email (explife365 at gmail)"
Write-Host "  Gate.io     listing@gate.io     sent $($OutreachSent.ToString('MMM d')) - await reply"
Write-Host "  MEXC        listing@mexc.com    sent $($OutreachSent.ToString('MMM d')) - await reply"
Write-Host "  KuCoin      listing@kucoin.com  sent $($OutreachSent.ToString('MMM d')) - await reply"
Write-Host "  Bybit       listing@bybit.com   sent $($OutreachSent.ToString('MMM d')) - likely blocked, use LinkedIn"
if ($Today -ge $FollowUpStart -and $Today -le $FollowUpEnd.AddDays(1)) {
    Write-Host "  >> DAY 5-7 WINDOW: run open_followup_drafts.ps1" -ForegroundColor Yellow
} else {
    $days = ($FollowUpStart - $Today).Days
    Write-Host ('  Follow-up window: Sep 19-21 (' + $days + ' days)')
}
Write-Host ""

Write-Host "## Community"
$discord = Join-Path $PSScriptRoot "discord_funding_followup.txt"
Write-Host "  Discord funding thread  MANUAL - paste $discord"
if (Test-Path $VideoPath) {
    Write-Host "  Video attach            ready"
} else {
    Write-Host "  Video attach            missing"
}
Write-Host ""

Write-Host "## Infra"
$stack = @("tn10-wrpc-live", "tn10-integrator-api", "tn10-outbox-receiver")
foreach ($proc in $stack) {
    $p = Get-Process $proc -ErrorAction SilentlyContinue
    if ($p) {
        Write-Host "  $proc  running pid=$($p.Id)"
    } else {
        Write-Host "  $proc  STOPPED"
    }
}
$gate = $null
try {
    $gateLine = cargo run --release --bin tn10-node-health -- --dual --min-healthy 2 --json 2>&1 |
        Where-Object { $_ -match '^\{' } |
        Select-Object -Last 1
    if ($gateLine) {
        $gate = $gateLine | ConvertFrom-Json
        if ($gate.healthy) {
            Write-Host "  owned-node gate  $($gate.healthyNodes)/$($gate.requiredHealthyNodes) green"
        } else {
            Write-Host "  owned-node gate  RED - start kaspad + host02 tunnel"
        }
    } else {
        Write-Host "  owned-node gate  RED (no JSON)"
    }
} catch {
    Write-Host "  owned-node gate  RED (nodes unreachable)"
}
$evidence = Get-ChildItem ".local\evidence\evidence_*.json" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if ($evidence) {
    $fresh = Test-FileFresh $evidence.FullName 72
    Write-Host "  evidence pack     $fresh - $($evidence.Name)"
} else {
    Write-Host "  evidence pack     missing - run integrator_evidence_pack.ps1"
}
Write-Host ""

Write-Host "## Actions today"
$actions = @()
if (-not (Get-Process tn10-integrator-api -ErrorAction SilentlyContinue)) {
    $actions += "Start stack: powershell -File scripts/integrator_stack.ps1 (after kaspad 2/2)"
}
if ($null -eq $gate -or -not $gate.healthy) {
    $actions += "Start nodes: powershell -File scripts/tn10_kaspad.ps1 + host02 tunnel"
}
$actions += "Paste Discord follow-up + attach video"
$actions += "Bybit: bybit_linkedin.txt (not listing@bybit.com)"
if ($Today -ge $FollowUpStart) {
    $actions += "CEX follow-up drafts: open_followup_drafts.ps1"
}
foreach ($a in $actions) { Write-Host "  - $a" }
