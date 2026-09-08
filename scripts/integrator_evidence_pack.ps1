# Collect integrator rehearsal evidence for Foundation / exchange reviewers.
# Output: .local/evidence/evidence_<timestamp>.txt and evidence_<timestamp>.json
# Not a CertiK submission. Not consensus evidence.

$ErrorActionPreference = "Continue"
$root = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $root ".local\evidence"
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$outFile = Join-Path $outDir "evidence_$stamp.txt"
$jsonFile = Join-Path $outDir "evidence_$stamp.json"

$env:TN10_MIN_HEALTHY = "2"
$env:TN10_OWNED_NODE_URLS = "ws://127.0.0.1:18210,ws://127.0.0.1:28210"

New-Item -ItemType Directory -Force -Path $outDir | Out-Null

function Write-Section($title) {
    $line = "`n========== $title ==========`n"
    Add-Content -Path $outFile -Value $line
    Write-Host $line
}

Push-Location $root
try {
    "kaspa-frontier-engine integrator evidence pack" | Set-Content -Path $outFile
    Add-Content -Path $outFile -Value "generated: $(Get-Date -Format o)"
    Add-Content -Path $outFile -Value "repo: $root"
    Add-Content -Path $outFile -Value "certik plan: scripts/certik_score_plan.md"
    Add-Content -Path $outFile -Value "json artifact: $jsonFile"

    Write-Section "SDK Toccata gate (computeBudget + SilverScript)"
    $sdkGateText = & python "$root\scripts\tn10_sdk_gate.py" --json 2>&1 | Out-String
    $sdkGateText | Add-Content -Path $outFile
    $sdkGateExit = $LASTEXITCODE
    Add-Content -Path $outFile -Value "tn10_sdk_gate exit code: $sdkGateExit"
    $sdkGateJson = $null
    try {
        $sdkGateJson = $sdkGateText | ConvertFrom-Json
    }
    catch {
        Add-Content -Path $outFile -Value "sdk gate json parse failed: $_"
    }

    Write-Section "roadmap + community asks (tn10-status)"
    & cargo run --quiet --release --bin tn10-status 2>&1 | Tee-Object -FilePath $outFile -Append
    if ($LASTEXITCODE -ne 0) {
        Add-Content -Path $outFile -Value "tn10-status exit code: $LASTEXITCODE"
    }

    Write-Section "TN10 IBD watch (laptop + replica)"
    & python "$root\scripts\tn10_ibd_watch.py" 2>&1 | Tee-Object -FilePath $outFile -Append
    $ibdExit = $LASTEXITCODE
    Add-Content -Path $outFile -Value "tn10_ibd_watch exit code: $ibdExit"

    Write-Section "TN10 IBD watch JSON"
    $ibdJsonText = & python "$root\scripts\tn10_ibd_watch.py" --json 2>&1 | Out-String
    $ibdJsonText | Add-Content -Path $outFile
    $ibdJson = $null
    try {
        $ibdJson = $ibdJsonText | ConvertFrom-Json
    }
    catch {
        Add-Content -Path $outFile -Value "ibd json parse failed: $_"
    }

    Write-Section "dual wrpc resnapshot (test address)"
    $wrpcArgs = @(
        "run", "--quiet", "--release", "--bin", "tn10-wrpc-live", "--",
        "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt",
        "--dual",
        "--resnapshot-only"
    )
    & cargo @wrpcArgs 2>&1 | Tee-Object -FilePath $outFile -Append
    Add-Content -Path $outFile -Value "tn10-wrpc-live resnapshot exit code: $LASTEXITCODE"

    Write-Section "L1 covenant proof offline (fixtures)"
    $proofPath = Join-Path $root "fixtures\tn10-counter-proof.json"
    $proofOfflineText = & cargo run --quiet --release --bin tn10-proof -- $proofPath --offline --json 2>&1 | Out-String
    $proofOfflineText | Add-Content -Path $outFile
    $proofOfflineExit = $LASTEXITCODE
    Add-Content -Path $outFile -Value "tn10-proof --offline exit code: $proofOfflineExit"

    Write-Section "L1 covenant proof live kascov (REST txids may be pruned)"
    $proofKascovText = & cargo run --quiet --release --bin tn10-proof -- $proofPath --kascov-only --json 2>&1 | Out-String
    $proofKascovText | Add-Content -Path $outFile
    $proofKascovExit = $LASTEXITCODE
    Add-Content -Path $outFile -Value "tn10-proof --kascov-only exit code: $proofKascovExit"

    Write-Section "L1 covenant proof live (REST + kascov)"
    $proofJsonText = & cargo run --quiet --release --bin tn10-proof -- $proofPath --json 2>&1 | Out-String
    $proofJsonText | Add-Content -Path $outFile
    $proofExit = $LASTEXITCODE
    Add-Content -Path $outFile -Value "tn10-proof live exit code: $proofExit"
    $proofJson = $null
    try {
        $proofJson = $proofJsonText | ConvertFrom-Json
    }
    catch {
        Add-Content -Path $outFile -Value "proof json parse failed: $_"
    }
    $proofOfflineJson = $null
    try {
        $proofOfflineJson = $proofOfflineText | ConvertFrom-Json
    }
    catch {
        Add-Content -Path $outFile -Value "offline proof json parse failed: $_"
    }
    $proofKascovJson = $null
    try {
        $proofKascovJson = $proofKascovText | ConvertFrom-Json
    }
    catch {
        Add-Content -Path $outFile -Value "kascov-only proof json parse failed: $_"
    }

    Write-Section "covenant RPC live smoke (REST + kascov)"
    $rpcSmokeText = & powershell -File "$root\scripts\tn10_covenant_rpc_smoke.ps1" -Json 2>&1 | Out-String
    $rpcSmokeText | Add-Content -Path $outFile
    $rpcSmokeExit = $LASTEXITCODE
    Add-Content -Path $outFile -Value "tn10_covenant_rpc_smoke exit code: $rpcSmokeExit"
    $rpcSmokeJson = $null
    try {
        $rpcSmokeJson = $rpcSmokeText | ConvertFrom-Json
    }
    catch {
        Add-Content -Path $outFile -Value "covenant rpc smoke json parse failed: $_"
    }

    Write-Section "owned-node health gate"
    $gateJsonText = & powershell -File "$root\scripts\tn10_gate.ps1" -Json 2>&1 | Out-String
    $gateJsonText | Add-Content -Path $outFile
    $healthExit = $LASTEXITCODE
    Add-Content -Path $outFile -Value "tn10_gate exit code: $healthExit"
    $gateJson = $null
    try {
        $gateJson = $gateJsonText | ConvertFrom-Json
    }
    catch {
        Add-Content -Path $outFile -Value "gate json parse failed: $_"
    }

    $combined = [ordered]@{
        generated = (Get-Date -Format o)
        repo = $root
        certikPlan = "scripts/certik_score_plan.md"
        tn10MinHealthy = $env:TN10_MIN_HEALTHY
        tn10OwnedNodeUrls = $env:TN10_OWNED_NODE_URLS
        sdkGate = $sdkGateJson
        ibdWatch = $ibdJson
        ownedNodeGate = $gateJson
        covenantProof = $proofJson
        covenantProofOffline = $proofOfflineJson
        covenantProofKascovOnly = $proofKascovJson
        covenantRpcSmoke = $rpcSmokeJson
        exitCodes = [ordered]@{
            sdkGate = $sdkGateExit
            ibdWatch = $ibdExit
            ownedNodeGate = $healthExit
            covenantProofOffline = $proofOfflineExit
            covenantProofKascovOnly = $proofKascovExit
            covenantProofLive = $proofExit
            covenantRpcSmoke = $rpcSmokeExit
        }
    }
    $combined | ConvertTo-Json -Depth 12 | Set-Content -Path $jsonFile -Encoding utf8
    Add-Content -Path $outFile -Value "`ncombined JSON: $jsonFile"

    Write-Section "cargo test --lib"
    & cargo test --lib 2>&1 | Tee-Object -FilePath $outFile -Append
    Add-Content -Path $outFile -Value "cargo test --lib exit code: $LASTEXITCODE"

    Write-Section "pytest tests/"
    & python -m pytest tests/ -q 2>&1 | Tee-Object -FilePath $outFile -Append
    Add-Content -Path $outFile -Value "pytest exit code: $LASTEXITCODE"

    Write-Host "`nWrote $outFile"
    Write-Host "Wrote $jsonFile"
}
finally {
    Pop-Location
}
