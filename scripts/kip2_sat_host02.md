# host02 DAGKnight SAT proofs

**Not a KIP-2 verdict.** Live L1 is GHOSTDAG @ 10 BPS. Do not patch kaspad.

- solver: `/opt/pvsnp/solvers/bin/kissat`
- checker: `/opt/pvsnp/solvers/bin/drat-trim`
- all_match: **True**
- DRAT verified: ['dk2_n6_T6_d1_f4.cnf', 'dk2_n6_T6_d2_f4.cnf', 'dk2_n8_T7_d1_f6.cnf', 'dk2_n8_T7_d2_f6.cnf', 'dk2_n10_T8_d1_f8.cnf', 'dk2_n10_T8_d2_f8.cnf']
- UNSAT without checked DRAT: none

| CNF | expect | got | match | secs | DRAT |
|-----|--------|-----|-------|------|------|
| `dk2_n6_T6_d1_f2.cnf` | SAT | SAT | True | 0.0072 | - |
| `dk2_n6_T6_d1_f4.cnf` | UNSAT | UNSAT | True | 0.0261 | verified |
| `dk2_n6_T6_d2_f4.cnf` | UNSAT | UNSAT | True | 0.0239 | verified |
| `dk2_n8_T7_d1_f2.cnf` | SAT | SAT | True | 0.0039 | - |
| `dk2_n8_T7_d1_f6.cnf` | UNSAT | UNSAT | True | 0.0295 | verified |
| `dk2_n8_T7_d2_f4.cnf` | SAT | SAT | True | 0.0037 | - |
| `dk2_n8_T7_d2_f6.cnf` | UNSAT | UNSAT | True | 0.0387 | verified |
| `dk2_n10_T8_d1_f2.cnf` | SAT | SAT | True | 0.0084 | - |
| `dk2_n10_T8_d1_f8.cnf` | UNSAT | UNSAT | True | 0.393 | verified |
| `dk2_n10_T8_d2_f4.cnf` | SAT | SAT | True | 0.0054 | - |
| `dk2_n10_T8_d2_f8.cnf` | UNSAT | UNSAT | True | 0.0751 | verified |
| `ghostdag_max_n6_k2.cnf` | SAT | SAT | True | 0.0022 | - |
| `ghostdag_max_n8_k2.cnf` | SAT | SAT | True | 0.0021 | - |

SAT rows are finite counterexamples to the *toy* f=2d encoding (except where noted).
UNSAT + verified DRAT is a proof that *that CNF* has no model.
It is not a proof of DAGKnight, KIP-2, or 100 BPS.

**host02 stack policy:** tuce orch, mamajama, flywheel, and engos stay off for this path.
Current toys are already DRAT-checked. Do not scp/ssh, do not resume satcomp grind, and
do not add those services as crate dependencies. Revisit only if a *new* UNSAT CNF
cannot be DRAT-checked on Windows PySAT — then use `/opt/pvsnp/solvers/bin/kissat`
and `drat-trim` only.
