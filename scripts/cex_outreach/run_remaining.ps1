# Operator checklist for remaining outreach tracks (manual steps flagged).

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $Root
$Outreach = Join-Path $PSScriptRoot

Write-Host "=== CEX / community remaining tracks ==="
Write-Host ""
Write-Host "[1] Stack (custody)"
Write-Host "    powershell -File scripts/integrator_stack.ps1"
Write-Host "    powershell -File scripts/integrator_stack.ps1 --status"
Write-Host ""
Write-Host "[2] Demo dashboard API"
Write-Host "    python examples/integrator_api.py"
Write-Host ""
Write-Host "[3] Discord funding thread (MANUAL paste)"
Write-Host "    File: scripts/cex_outreach/discord_funding_followup.txt"
Write-Host ""
Write-Host "[4] Bybit alternate (MANUAL if listing@ bounced)"
Write-Host "    File: scripts/cex_outreach/bybit_linkedin.txt"
Write-Host "    Form: https://www.bybit.com/en/help-center/"
Write-Host ""
Write-Host "[5] CEX day 5-7 follow-up (Sep 19-21 2026 if no reply)"
Write-Host "    File: scripts/cex_outreach/followup_day5.txt"
Write-Host "    Drafts: scripts/cex_outreach/open_followup_drafts.ps1"
Write-Host ""
Write-Host "[6] Jackpot draw when round ends"
Write-Host "    powershell -File scripts/galleon_jackpot_draw_when_ready.ps1"
Write-Host ""
Write-Host "[7] Dev video — DONE (attach to Discord + CEX replies when sending)"
Write-Host ""
Write-Host "[8] Fresh evidence before CEX call"
Write-Host "    powershell -File scripts/integrator_evidence_pack.ps1"
Write-Host ""

if ($args -contains "--open-drafts") {
    & (Join-Path $Outreach "open_gmail_drafts.ps1")
}
