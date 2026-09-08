# Poll node 1 + node 2 TN10 owned nodes. See scripts/tn10_ibd_watch.py
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
python "$root\scripts\tn10_ibd_watch.py"
