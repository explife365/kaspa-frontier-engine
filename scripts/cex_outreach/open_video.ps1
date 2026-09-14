# Open the rendered dev video folder (for Discord / CEX attachment).

$VideoRoot = "C:\Users\Admin2\IONOS HiDrive\Build Files\Kaspa Video"
$Final = Join-Path $VideoRoot "out\kaspa_dev_quickstart.mp4"

if (Test-Path $Final) {
    Write-Host "Opening $Final"
    Start-Process $Final
} else {
    Write-Host "Final render not found: $Final"
    if (Test-Path $VideoRoot) {
        Start-Process (Join-Path $VideoRoot "out")
    }
}
