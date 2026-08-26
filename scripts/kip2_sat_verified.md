# DAGKnight v2 - independently verified (still not KIP-2)

**This does not ratify KIP-2.** Live Kaspa L1 is GHOSTDAG @ 10 BPS. These checks are about a finite toy encoding of a stated delay hypothesis.

What is proven here:

- **SAT** instances have a DAG witness that independently violates `honest anticone pair with |t(u)-t(v)|<=d => A(.)<=f`.
- Those witnesses were re-found by Glucose4, Cadical195, and Minisat22; each SAT model satisfies every DIMACS clause. (Kissat404 via PySAT access-violates on this Windows Python, so it is not used.)
- **UNSAT** at the f-sweep boundary (n=6 f=4, n=8 f=6, n=10 f=8, both d=1 and d=2) is agreed by those three independent CDCL solvers.
- SAT models are the checkable certificates. UNSAT is three-solver agreement, not a machine-checked DRAT proof (PySAT traces are not plain RUP).

That is a verified statement about *this encoding at these n*, not a protocol proof and not an activation argument.

## Solver agreement

| CNF | logged | agreed | Glucose4 | Cadical195 | Minisat22 |
|-----|--------|--------|----------|------------|-----------|
| dk2_n6_T6_d1_f2.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| dk2_n6_T6_d1_f3.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| dk2_n6_T6_d1_f4.cnf | UNSAT | UNSAT | UNSAT | UNSAT | UNSAT |
| dk2_n6_T6_d2_f3.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| dk2_n6_T6_d2_f4.cnf | UNSAT | UNSAT | UNSAT | UNSAT | UNSAT |
| dk2_n8_T7_d1_f2.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| dk2_n8_T7_d1_f5.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| dk2_n8_T7_d1_f6.cnf | UNSAT | UNSAT | UNSAT | UNSAT | UNSAT |
| dk2_n8_T7_d2_f4.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| dk2_n8_T7_d2_f5.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| dk2_n8_T7_d2_f6.cnf | UNSAT | UNSAT | UNSAT | UNSAT | UNSAT |
| dk2_n10_T8_d1_f2.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| dk2_n10_T8_d1_f7.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| dk2_n10_T8_d1_f8.cnf | UNSAT | UNSAT | UNSAT | UNSAT | UNSAT |
| dk2_n10_T8_d2_f4.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| dk2_n10_T8_d2_f7.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| dk2_n10_T8_d2_f8.cnf | UNSAT | UNSAT | UNSAT | UNSAT | UNSAT |
| ghostdag_max_n6_k2.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| ghostdag_max_n8_k2.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |
| ghostdag_max_n8_k3.cnf | SAT | SAT | SAT+model | SAT+model | SAT+model |

## Reading

- Stated `f=2d` has a checkable counterexample except `n=6 d=2`.
- Finite safe `f` at these sizes is `n-2`. That tracks graph size, not delay.
- Do not treat this as a KIP-2 `k`. Do not patch kaspad. Do not emit 100 BPS telemetry.

## How a KIP-2 verdict actually happens

`kaspanet/kips` `kip-0002.md` is still **Status: Proposed**. It is a consensus hard fork plus RPC. A verdict is not a SAT result.

1. Applied research in the KIP (efficient DK, global latency bound for difficulty/pruning) finishes.
2. DK is implemented in rusty-kaspa (GHOSTDAG remains a subroutine). Wallet confirmation-policy RPC exists.
3. Staged nets: internal v0 devnet (in progress) → public v1 testnet → v2 mainnet candidate.
4. `kip-0002.md` Status leaves Proposed. Independent nodes agree.
5. Mainnet hard-fork activation (DAA/time). Live ordering is then DK, not GHOSTDAG @ 10 BPS.

This crate can post toy SAT/DRAT to the KIP thread. It cannot merge rusty-kaspa, flip KIP status, or activate a HF. Fake 100 BPS telemetry is not a verdict.


Source verifier: `AI Agent/scripts/dagknight_sat/verify_round3.py`
Artifacts: `AI Agent/scripts/dagknight_sat/out_v2/verified/`
