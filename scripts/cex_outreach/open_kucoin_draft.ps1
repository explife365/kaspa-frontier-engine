# Reopen only the KuCoin compose (explife365@gmail.com).
$GmailUser = "explife365@gmail.com"
$path = Join-Path $PSScriptRoot "kucoin_integrations.txt"
$lines = Get-Content $path
$to = ($lines | Where-Object { $_ -match '^To:\s*(.+)$' } | ForEach-Object { $Matches[1] } | Select-Object -First 1)
$subject = ($lines | Where-Object { $_ -match '^Subject:\s*(.+)$' } | ForEach-Object { $Matches[1] } | Select-Object -First 1)
$bodyStart = [array]::IndexOf($lines, ($lines | Where-Object { $_ -match '^Subject:' } | Select-Object -First 1)) + 1
$body = ($lines[$bodyStart..($lines.Length - 1)] | Where-Object { $_.Trim().Length -gt 0 }) -join "`n"
$url = "https://mail.google.com/mail/?authuser=$([uri]::EscapeDataString($GmailUser))&view=cm&fs=1&to=$([uri]::EscapeDataString($to))&su=$([uri]::EscapeDataString($subject))&body=$([uri]::EscapeDataString($body))"
Write-Host "Opening KuCoin draft as $GmailUser ..."
Start-Process $url
