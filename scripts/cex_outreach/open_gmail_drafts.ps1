# Open Gmail compose windows for CEX outreach drafts.
# Pins authuser=explife365@gmail.com so drafts do not land on the default signed-in account.
# Draft text: scripts/cex_outreach/*.txt

$GmailUser = "explife365@gmail.com"

$files = @(
    "mexc_integrations.txt",
    "kucoin_integrations.txt",
    "gateio_integrations.txt",
    "bybit_integrations.txt"
)

$root = Split-Path -Parent $MyInvocation.MyCommand.Path

foreach ($file in $files) {
    $path = Join-Path $root $file
    $lines = Get-Content $path
    $to = ($lines | Where-Object { $_ -match '^To:\s*(.+)$' } | ForEach-Object { $Matches[1] } | Select-Object -First 1)
    $subject = ($lines | Where-Object { $_ -match '^Subject:\s*(.+)$' } | ForEach-Object { $Matches[1] } | Select-Object -First 1)
    $bodyStart = [array]::IndexOf($lines, ($lines | Where-Object { $_ -match '^Subject:' } | Select-Object -First 1)) + 1
    $body = ($lines[$bodyStart..($lines.Length - 1)] | Where-Object { $_.Trim().Length -gt 0 }) -join "`n"
    $url = "https://mail.google.com/mail/?authuser=$([uri]::EscapeDataString($GmailUser))&view=cm&fs=1&to=$([uri]::EscapeDataString($to))&su=$([uri]::EscapeDataString($subject))&body=$([uri]::EscapeDataString($body))"
    Write-Host "Opening $to as $GmailUser ..."
    Start-Process $url
    Start-Sleep -Seconds 2
}

Write-Host "Done. Save each as Gmail draft (review before send)."
