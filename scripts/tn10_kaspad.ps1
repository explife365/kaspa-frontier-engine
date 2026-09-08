# Start the owned TN10 kaspad. Loopback RPC only. Always uses the tn10 appdir.
# Does not replace a running node. Does not bind 18210/16210 on public interfaces.
$ErrorActionPreference = "Stop"
$exe = Join-Path $env:LOCALAPPDATA "kaspa\v2.0.1\kaspad.exe"
$appdir = Join-Path $env:LOCALAPPDATA "kaspa\tn10"
if (-not (Test-Path $exe)) {
    throw "missing $exe"
}
$existing = Get-CimInstance Win32_Process -Filter "Name='kaspad.exe'"
if ($existing) {
    throw "kaspad already running (pid $($existing.ProcessId)); not starting a second copy"
}
Write-Host "starting $exe --appdir $appdir (RPC 127.0.0.1:16210/18210, P2P 16211)"
& $exe --yes --testnet --netsuffix=10 --utxoindex --disable-upnp --rpclisten=127.0.0.1:16210 --rpclisten-json=127.0.0.1:18210 --listen=0.0.0.0:16211 --appdir=$appdir
