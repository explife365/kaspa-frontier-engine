# Rotate VPN egress for Galleon faucet relay.
# Usage:
#   powershell -File scripts/galleon_ip_rotate.ps1 -Backend hotspot-shield
#   powershell -File scripts/galleon_ip_rotate.ps1 -Backend mullvad -Location us

param(
    [ValidateSet("hotspot-shield", "mullvad", "nordvpn", "windscribe")]
    [string]$Backend = "hotspot-shield",
    [string]$Location = ""
)

$ErrorActionPreference = "Stop"

function Find-HssCp {
    $candidates = @(
        "${env:ProgramFiles}\Hotspot Shield\*\bin\hsscp.exe",
        "${env:ProgramFiles(x86)}\Hotspot Shield\*\bin\hsscp.exe"
    )
    foreach ($pattern in $candidates) {
        $hit = Get-ChildItem -Path $pattern -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1
        if ($hit) { return $hit.FullName }
    }
    return $null
}

function Rotate-HotspotShield {
    Write-Host "hotspot-shield: disconnect"
    Stop-Process -Name hydra -Force -ErrorAction SilentlyContinue
    Stop-Process -Name hsscp -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 5

    $exe = Find-HssCp
    if (-not $exe) {
        throw "Hotspot Shield hsscp.exe not found; install HSS or set GALLEON_VPN_BACKEND=mullvad"
    }
    Write-Host "hotspot-shield: connect via $exe"
    & $exe -connect
    Start-Sleep -Seconds 12

    $tap = Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object {
        $_.InterfaceDescription -match "Hotspot|TAP|Anchor"
    } | Select-Object -First 1
    if ($tap) {
        Write-Host "adapter $($tap.Name) status $($tap.Status)"
    }
}

function Find-WindscribeCli {
    $candidates = @(
        "${env:ProgramFiles}\Windscribe\windscribe-cli.exe",
        "${env:ProgramFiles(x86)}\Windscribe\windscribe-cli.exe"
    )
    foreach ($path in $candidates) {
        if (Test-Path $path) { return $path }
    }
    return $null
}

function Require-Cli([string]$Name) {
    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    if (-not $cmd) {
        throw "$Name CLI not found on PATH"
    }
    return $cmd.Source
}

function Rotate-Mullvad {
    Require-Cli "mullvad" | Out-Null
    mullvad disconnect | Out-Null
    Start-Sleep -Seconds 2
    if ($Location) {
        Write-Host "mullvad relay set location $Location"
        mullvad relay set location $Location | Out-Null
    }
    mullvad connect | Out-Null
    mullvad status -w | Out-Null
}

function Rotate-NordVpn {
    Require-Cli "nordvpn" | Out-Null
    nordvpn disconnect | Out-Null
    Start-Sleep -Seconds 2
    if ($Location) {
        nordvpn connect $Location | Out-Null
    } else {
        nordvpn connect | Out-Null
    }
}

function Rotate-Windscribe {
    $exe = Find-WindscribeCli
    if (-not $exe) {
        throw "windscribe-cli.exe not found under Program Files\Windscribe"
    }
    Write-Host "windscribe: disconnect via $exe"
    & $exe disconnect | Out-Null
    Start-Sleep -Seconds 2
    if ($Location) {
        Write-Host "windscribe: connect $Location"
        & $exe connect $Location | Out-Null
    } else {
        Write-Host "windscribe: connect (auto)"
        & $exe connect | Out-Null
    }
    Start-Sleep -Seconds 10
}

switch ($Backend) {
    "hotspot-shield" { Rotate-HotspotShield }
    "mullvad"        { Rotate-Mullvad }
    "nordvpn"        { Rotate-NordVpn }
    "windscribe"     { Rotate-Windscribe }
    default          { throw "unknown backend $Backend" }
}

Write-Host "vpn rotate ok ($Backend)"
