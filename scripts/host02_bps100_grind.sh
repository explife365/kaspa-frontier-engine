#!/bin/bash
# 100 BPS lore packing on host02. NOT live telemetry. NOT KIP-2. NOT satcomp.
set -euo pipefail
KISSAT=/opt/pvsnp/solvers/bin/kissat
DRAT=/opt/pvsnp/solvers/bin/drat-trim
REMOTE=/opt/pvsnp/saas/solvers/dagknight_sat/bps100
cd "$REMOTE"

echo "=== pause satcomp2025 grind (free cores for 100 BPS lore SAT) ==="
systemctl stop tuce-satcomp2025-orch || true
systemctl is-active tuce-satcomp2025-orch || true
systemctl list-units --type=service --state=running --no-legend 'tuce-mamajama-*' --plain --no-pager 2>/dev/null \
  | awk '{print $1}' \
  | while read -r u; do
      [ -n "$u" ] && systemctl stop "$u" 2>/dev/null || true
    done
echo "orch=$(systemctl is-active tuce-satcomp2025-orch 2>/dev/null || true)"
echo "mamajama_left=$(systemctl list-units --type=service --state=running --no-legend 'tuce-mamajama-*' 2>/dev/null | wc -l)"

echo "=== kissat proof flags ==="
"$KISSAT" --help 2>&1 | grep -Ei 'proof|unsat|binary' | head -40 || true

solve_one() {
  local inst="$1"
  echo "=== kissat $inst ==="
  set +e
  "$KISSAT" --no-binary "${inst}.cnf" "${inst}.drat" > "${inst}.log" 2>&1
  local rc=$?
  set -e
  echo "kissat_rc=$rc"
  tail -n 20 "${inst}.log" || true
  if [ -s "${inst}.drat" ]; then
    set +e
    "$DRAT" "${inst}.cnf" "${inst}.drat" > "${inst}.drat.log" 2>&1
    echo "drat_rc=$?"
    set -e
    tail -n 8 "${inst}.drat.log" || true
  else
    echo "NO_DRAT $inst"
  fi
}

solve_one ghostdag_k18_bps10_L1
solve_one ghostdag_k18_bps100_L1
solve_one ghostdag_k100_bps100_L1

if pgrep -f 'kissat.*ghostdag_k18_bps100_L1_fulln' >/dev/null; then
  echo "FULLN_ALREADY_RUNNING"
else
  nohup "$KISSAT" --no-binary ghostdag_k18_bps100_L1_fulln.cnf ghostdag_k18_bps100_L1_fulln.drat \
    > ghostdag_k18_bps100_L1_fulln.log 2>&1 &
  echo $! > kissat_fulln.pid
  echo "FULLN_PID=$(cat kissat_fulln.pid)"
fi
sleep 2
echo "=== fulln head ==="
head -n 15 ghostdag_k18_bps100_L1_fulln.log || true
if [ -f kissat_fulln.pid ]; then
  ps -p "$(cat kissat_fulln.pid)" -o pid,etime,cmd || true
fi
{
  echo "# host02 100 BPS lore grind"
  echo
  echo "Not live telemetry. Live L1 remains GHOSTDAG @ 10 BPS."
  echo "Not KIP-2. Not TN12. Not Rothschild TPS. Not satcomp mamajama."
  echo "Started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "fulln pid: $(cat kissat_fulln.pid 2>/dev/null || echo none)"
} > HOST02_PROOF.md
echo GRIND_STARTED
