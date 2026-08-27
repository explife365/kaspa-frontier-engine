# Rehearsal mTLS material for the loopback outbox receiver.
# Writes gitignored PEMs under .local/tn10-mtls/. Not for production CAs.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$out = Join-Path $root ".local\tn10-mtls"
$openssl = @(
    "$env:ProgramFiles\Git\usr\bin\openssl.exe",
    "openssl"
) | Where-Object {
    if ($_ -eq "openssl") { Get-Command openssl -ErrorAction SilentlyContinue } else { Test-Path $_ }
} | Select-Object -First 1
if (-not $openssl) {
    throw "openssl is required (Git for Windows includes usr\bin\openssl.exe)"
}

New-Item -ItemType Directory -Force -Path $out | Out-Null
$caExt = Join-Path $out "ca.ext"
$serverExt = Join-Path $out "server.ext"
$clientExt = Join-Path $out "client.ext"
Set-Content -Path $caExt -Value "basicConstraints=critical,CA:TRUE`nkeyUsage=critical,keyCertSign,cRLSign"
Set-Content -Path $serverExt -Value "subjectAltName=IP:127.0.0.1,DNS:localhost`nkeyUsage=critical,digitalSignature,keyEncipherment`nextendedKeyUsage=serverAuth"
Set-Content -Path $clientExt -Value "keyUsage=critical,digitalSignature`nextendedKeyUsage=clientAuth"

& $openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj "/CN=tn10-receiver-ca" -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign" -keyout (Join-Path $out "ca.key") -out (Join-Path $out "ca.pem")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& $openssl req -newkey rsa:2048 -nodes -subj "/CN=127.0.0.1" -keyout (Join-Path $out "server.key") -out (Join-Path $out "server.csr")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& $openssl x509 -req -in (Join-Path $out "server.csr") -CA (Join-Path $out "ca.pem") -CAkey (Join-Path $out "ca.key") -CAcreateserial -out (Join-Path $out "server.pem") -days 2 -extfile $serverExt
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& $openssl req -newkey rsa:2048 -nodes -subj "/CN=tn10-outbox" -keyout (Join-Path $out "client.key") -out (Join-Path $out "client.csr")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& $openssl x509 -req -in (Join-Path $out "client.csr") -CA (Join-Path $out "ca.pem") -CAkey (Join-Path $out "ca.key") -CAcreateserial -out (Join-Path $out "client.pem") -days 2 -extfile $clientExt
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "wrote $out"
Write-Host "receiver: --tls-cert .local/tn10-mtls/server.pem --tls-key .local/tn10-mtls/server.key --tls-client-ca .local/tn10-mtls/ca.pem"
Write-Host "deliver:  --tls-ca .local/tn10-mtls/ca.pem --tls-cert .local/tn10-mtls/client.pem --tls-key .local/tn10-mtls/client.key"
