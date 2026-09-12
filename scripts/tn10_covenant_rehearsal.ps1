# TN10 L1 covenant integrator rehearsal (read + verify; native broadcast fail-closed until SDK PR 78).
# Not consensus evidence. Uses checked-in SilverScript counter proof bundle.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$proof = Join-Path $root "fixtures\tn10-counter-proof.json"

Push-Location $root
try {
    Write-Host "=== SilverScript compile + SDK computeBudget gate ==="
    if ($env:TN10_SDK_DEV_PATCH -eq "1") {
        python "$root\scripts\tn10_sdk_gate.py" --dev --json
    } else {
        python "$root\scripts\tn10_sdk_gate.py" --json
    }
    if ($LASTEXITCODE -eq 0) {
        Write-Host "SDK gate green - covenant broadcast may proceed"
    }
    else {
        Write-Host "SDK gate red - covenant broadcast remains fail-closed (see scripts/sdk_pr78_comment.md)"
    }
    python -m pytest tests/test_silverscript_compile.py -q
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "SilverScript/SDK gate failed"
        exit $LASTEXITCODE
    }

    Write-Host "`n=== offline proof bundle (REST + kascov fixtures) ==="
    cargo run --release --bin tn10-proof -- $proof --offline --json
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host "`n=== covenant RPC unit tests ==="
    cargo test --lib covenant_rpc::tests -- --nocapture
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host "`n=== covenant RPC live smoke (REST + kascov) ==="
    powershell -File "$root\scripts\tn10_covenant_rpc_smoke.ps1"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host "`n=== live TN10 proof verify (kascov; REST txids may be pruned) ==="
    cargo run --release --bin tn10-proof -- $proof --kascov-only --json
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "live kascov proof verify failed - check kascov reachability"
    }

    Write-Host "`n=== live TN10 proof verify (full REST + kascov; may fail if txids pruned) ==="
    cargo run --release --bin tn10-proof -- $proof --json
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "live proof verify failed - regenerate with counter.py after SDK #78 or if TN10 pruned old txids"
    }

    Write-Host "`n=== timelock vault compile + offline proof ==="
    python -c @"
import sys
from pathlib import Path
sys.path.insert(0, str(Path(r'$root') / 'examples' / 'silverscript'))
import timelock_vault
sample = timelock_vault.compiled_vault(1_000_000)
print(f'timelock vault script bytes: {len(bytes(sample.script))}')
try:
    timelock_vault.require_toccata_sdk()
    print('SDK computeBudget OK - vault broadcast may proceed')
except RuntimeError as e:
    print(f'vault broadcast blocked: {e}')
"@

    $vaultProof = Join-Path $root "fixtures\tn10-vault-proof.json"
    if (Test-Path $vaultProof) {
        Write-Host "`n=== timelock vault proof offline (fixtures) ==="
        cargo run --release --bin tn10-proof -- $vaultProof --offline --json
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    else {
        Write-Host "`n=== timelock vault proof (skipped - no fixtures/tn10-vault-proof.json) ==="
    }

    Write-Host "`n=== restricted swap compile + offline proof ==="
    python -c @"
import sys
from pathlib import Path
sys.path.insert(0, str(Path(r'$root') / 'examples' / 'silverscript'))
import restricted_swap
sample = restricted_swap.compiled_swap(restricted_swap.ALLOWED_RECIPIENT_HASH)
print(f'restricted swap script bytes: {len(bytes(sample.script))}')
try:
    restricted_swap.require_toccata_sdk()
    print('SDK computeBudget OK - swap broadcast may proceed')
except RuntimeError as e:
    print(f'swap broadcast blocked: {e}')
"@

    $swapProof = Join-Path $root "fixtures\tn10-swap-proof.json"
    if (Test-Path $swapProof) {
        Write-Host "`n=== restricted swap proof offline (fixtures) ==="
        cargo run --release --bin tn10-proof -- $swapProof --offline --json
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    else {
        Write-Host "`n=== restricted swap proof (skipped - no fixtures/tn10-swap-proof.json) ==="
    }

    Write-Host "`n=== HTLC compile + offline proof ==="
    python -c @"
import sys
from pathlib import Path
sys.path.insert(0, str(Path(r'$root') / 'examples' / 'silverscript'))
import htlc
sample = htlc.compiled_htlc(htlc.PAYMENT_HASH, 2_000_000)
print(f'HTLC script bytes: {len(bytes(sample.script))}')
try:
    htlc.require_toccata_sdk()
    print('SDK computeBudget OK - htlc.py broadcast may proceed')
except RuntimeError as e:
    print(f'HTLC broadcast blocked: {e}')
"@

    $htlcProof = Join-Path $root "fixtures\tn10-htlc-proof.json"
    if (Test-Path $htlcProof) {
        Write-Host "`n=== HTLC proof offline (fixtures) ==="
        cargo run --release --bin tn10-proof -- $htlcProof --offline --json
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    else {
        Write-Host "`n=== HTLC proof (skipped - no fixtures/tn10-htlc-proof.json) ==="
    }

    $htlcRefundProof = Join-Path $root "fixtures\tn10-htlc-refund-proof.json"
    if (Test-Path $htlcRefundProof) {
        Write-Host "`n=== HTLC refund proof offline (fixtures) ==="
        cargo run --release --bin tn10-proof -- $htlcRefundProof --offline --json
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    else {
        Write-Host "`n=== HTLC refund proof (skipped - no fixtures/tn10-htlc-refund-proof.json) ==="
    }

    Write-Host "`n=== 2-of-3 escrow compile + offline proof ==="
    python -c @"
import sys
from pathlib import Path
sys.path.insert(0, str(Path(r'$root') / 'examples' / 'silverscript'))
import escrow_2of3
sample = escrow_2of3.compiled_escrow(
    escrow_2of3.BUYER_HASH,
    escrow_2of3.SELLER_HASH,
    escrow_2of3.ARBITER_HASH,
    2_000_000,
)
print(f'escrow script bytes: {len(bytes(sample.script))}')
try:
    escrow_2of3.require_toccata_sdk()
    print('SDK computeBudget OK - escrow_2of3.py broadcast may proceed')
except RuntimeError as e:
    print(f'escrow broadcast blocked: {e}')
"@

    $escrowProof = Join-Path $root "fixtures\tn10-escrow-2of3-proof.json"
    if (Test-Path $escrowProof) {
        Write-Host "`n=== escrow 2-of-3 proof offline (fixtures) ==="
        cargo run --release --bin tn10-proof -- $escrowProof --offline --json
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    else {
        Write-Host "`n=== escrow 2-of-3 proof (skipped - no fixtures/tn10-escrow-2of3-proof.json) ==="
    }

    Write-Host "`n=== fresh counter broadcast path (native fail-closed until SDK PR 78) ==="
    python -c @"
import sys
from pathlib import Path
sys.path.insert(0, str(Path(r'$root') / 'examples' / 'silverscript'))
import counter
try:
    counter.require_toccata_sdk()
    print('SDK computeBudget OK - counter.py broadcast may proceed')
except RuntimeError as e:
    print(f'broadcast blocked: {e}')
"@
}
finally {
    Pop-Location
}
