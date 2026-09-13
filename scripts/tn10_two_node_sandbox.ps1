# TN10 two-node sandbox: 2/2 gate, test transfer, dual-node UTXO check, proof bundle.
# Not consensus evidence.
param(
    [switch]$DryRun,
    [switch]$Transfer,
    [switch]$Covenant,
    [decimal]$Kas = 0.2,
    [string]$From = "alice",
    [string]$To = "bob",
    [switch]$Json
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$env:TN10_MIN_HEALTHY = "2"
$env:TN10_OWNED_NODE_URLS = "ws://127.0.0.1:18210,ws://127.0.0.1:28210"

Push-Location $root
try {
    $args = @("$root\examples\tn10_two_node_sandbox.py")
    if ($DryRun) { $args += "--dry-run" }
    if ($Transfer) { $args += "--transfer" }
    if ($Covenant) { $args += "--covenant" }
    if ($Json) { $args += "--json" }
    $args += @("--kas", "$Kas", "--from", $From, "--to", $To)
    & python @args
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
