# Loopback-only wRPC tunnel to the host02 TN10 replica.
# Reconnects after SSH resets. Does not expose 18210 on the public internet.
$ErrorActionPreference = "Stop"
$key = Join-Path $env:USERPROFILE ".ssh\tuce_mgmt"
if (-not (Test-Path $key)) {
    throw "missing SSH key $key"
}

$backoff = 2
$immediateFailures = 0
while ($true) {
    Write-Host "forwarding 127.0.0.1:28210 -> host02.tuce.app 127.0.0.1:18210"
    $started = Get-Date
    & ssh -i $key -N -o BatchMode=yes -o IdentitiesOnly=yes -o ExitOnForwardFailure=yes -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o TCPKeepAlive=yes -L 127.0.0.1:28210:127.0.0.1:18210 root@host02.tuce.app
    $code = $LASTEXITCODE
    $elapsed = ((Get-Date) - $started).TotalSeconds
    if ($elapsed -lt 5) {
        $immediateFailures++
        if ($immediateFailures -ge 3) {
            throw "ssh failed immediately (exit $code); not retrying"
        }
    } else {
        $immediateFailures = 0
        $backoff = 2
    }
    Write-Host "tunnel exited $code after ${elapsed}s; retrying in ${backoff}s"
    Start-Sleep -Seconds $backoff
    $backoff = [Math]::Min(($backoff * 2), 30)
}
