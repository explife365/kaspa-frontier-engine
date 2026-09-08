# Fail-closed dual owned-node health gate (laptop + host02 replica).
# Exit 0 only when --min-healthy 2 passes on 18210 + 28210.
param(
    [switch]$Json
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $cargoArgs = @("run", "--release", "--bin", "tn10-node-health", "--", "--dual", "--min-healthy", "2")
    if ($Json) {
        $cargoArgs = @("run", "--quiet", "--release", "--bin", "tn10-node-health", "--", "--dual", "--min-healthy", "2", "--json")
    }
    & cargo @cargoArgs
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
