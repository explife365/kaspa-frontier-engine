# Live smoke for tn10-covenant-rpc (REST + kascov). Not kaspad. Not consensus evidence.
param(
    [switch]$Json,
    [string]$CovenantId = "4a95a59dc79c3f46f35db91453f26785750450606836d82c48c1affdd71ed70a"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$logDir = Join-Path $root ".local"
$stdoutLog = Join-Path $logDir "covenant-rpc-smoke-out.txt"
$stderrLog = Join-Path $logDir "covenant-rpc-smoke-err.txt"

New-Item -ItemType Directory -Force -Path $logDir | Out-Null

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = ($listener.LocalEndpoint).Port
    $listener.Stop()
    return $port
}

function Invoke-Rpc($bind, $body) {
    $uri = "http://$bind/"
    return Invoke-RestMethod -Uri $uri -Method POST -Body $body -ContentType "application/json" -TimeoutSec 30
}

Push-Location $root
try {
    $port = Get-FreePort
    $bind = "127.0.0.1:$port"
    $proc = Start-Process -FilePath "cargo" -ArgumentList @(
        "run", "--quiet", "--release", "--bin", "tn10-covenant-rpc", "--", "--bind", $bind
    ) -PassThru -NoNewWindow -RedirectStandardOutput $stdoutLog -RedirectStandardError $stderrLog

    $ready = $false
    for ($i = 0; $i -lt 40; $i++) {
        Start-Sleep -Milliseconds 500
        try {
            $info = Invoke-Rpc $bind '{"jsonrpc":"2.0","id":1,"method":"getInfo","params":[]}'
            if ($info.result.notKaspad -eq $true) {
                $ready = $true
                break
            }
        }
        catch {
            if (-not $proc.HasExited) { continue }
            $tail = ""
            if (Test-Path $stderrLog) { $tail = (Get-Content $stderrLog -Tail 20 | Out-String) }
            throw "tn10-covenant-rpc exited before ready. $tail"
        }
    }
    if (-not $ready) {
        throw "tn10-covenant-rpc did not become ready on $bind within 20s"
    }

    $covenantParams = "{`"covenantId`":`"$CovenantId`"}"
    $covenant = Invoke-Rpc $bind "{`"jsonrpc`":`"2.0`",`"id`":2,`"method`":`"getCovenant`",`"params`":$covenantParams}"
    $utxos = Invoke-Rpc $bind "{`"jsonrpc`":`"2.0`",`"id`":3,`"method`":`"getUtxosByCovenantId`",`"params`":$covenantParams}"

    if ($covenant.error) { throw "getCovenant failed: $($covenant.error.message)" }
    if ($utxos.error) { throw "getUtxosByCovenantId failed: $($utxos.error.message)" }
    if ($covenant.result.covenant_id -ne $CovenantId) {
        throw "getCovenant covenant_id mismatch"
    }
    if (-not $covenant.result.lineage_complete) {
        throw "getCovenant lineage_complete is false"
    }

    $summary = [ordered]@{
        bind = $bind
        covenantId = $CovenantId
        notKaspad = $info.result.notKaspad
        implementation = $info.result.implementation
        lineageComplete = $covenant.result.lineage_complete
        liveUtxos = $covenant.result.live_utxos
        utxoCount = @($utxos.result.utxos).Count
        utxosVerified = $utxos.result.verified
        backend = $covenant.result.backend
    }

    if ($Json) {
        $summary | ConvertTo-Json -Depth 6
    }
    else {
        Write-Host "tn10-covenant-rpc smoke OK on $bind"
        Write-Host "  getInfo notKaspad=$($summary.notKaspad)"
        Write-Host "  getCovenant lineage_complete=$($summary.lineageComplete) live_utxos=$($summary.liveUtxos)"
        Write-Host "  getUtxosByCovenantId utxos=$($summary.utxoCount) verified=$($summary.utxosVerified)"
    }
    exit 0
}
catch {
    if ($Json) {
        @{ error = $_.Exception.Message; bind = $bind } | ConvertTo-Json
    }
    else {
        Write-Error $_
    }
    exit 1
}
finally {
    if ($proc -and -not $proc.HasExited) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }
    Get-Process -Name tn10-covenant-rpc -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Pop-Location
}
