# Open Gmail compose for day 5-7 CEX follow-up (all four exchanges).
# Uses followup_day5.txt body; customize To per exchange before send.

$GmailUser = "explife365@gmail.com"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$body = (Get-Content (Join-Path $Root "followup_day5.txt") | Select-Object -Skip 2) -join "`n"
$subject = "Re: Kaspa TN10 integrator pilot"

$recipients = @(
    "listing@gate.io",
    "listing@mexc.com",
    "listing@kucoin.com"
)

foreach ($to in $recipients) {
    $url = "https://mail.google.com/mail/?authuser=$([uri]::EscapeDataString($GmailUser))&view=cm&fs=1&to=$([uri]::EscapeDataString($to))&su=$([uri]::EscapeDataString($subject))&body=$([uri]::EscapeDataString($body))"
    Write-Host "Opening follow-up draft to $to ..."
    Start-Process $url
    Start-Sleep -Seconds 2
}

Write-Host "Bybit: use bybit_linkedin.txt (do not resend listing@bybit.com from Gmail)."
Write-Host "Save each as draft; send Sep 19-21 if no reply."
