//! Toy SAT fragments for KIP-2 / DAGKnight *discussion*.
//!
//! v2 SAT is **logged, not ratified**. Does not change live TN10 (GHOSTDAG @ 10 BPS).
//! Send findings to the KIP thread, not to mainnet.
//! Round 3 (Glucose4 + Cadical195 + Minisat22, models independently checked):
//! `scripts/kip2_sat_round3.txt` and `scripts/kip2_sat_verified.md`.

/// Solver used for v1/v2 local sweeps.
pub const ROUND1_SOLVER: &str = "pysat:Glucose4";

/// Encoding gap: Task A CNF was k-cluster only, not greedy maximality.
pub const ROUND1_TASK_A_GAP: &str =
    "Task A SAT under-colors (blues=[0]); greedy topo on the same DAGs paints all blue";

/// Toy encoding of f(d)=2d had SAT counterexamples at n<=12.
pub const ROUND1_TASK_B: &str =
    "Task B SAT at n=8 d=1 f=2 and n=12; counterexamples to this toy f(d)=2d, not a KIP-2 verdict";

/// Why v2 exists: topo-id-as-time made f=2d vacuously UNSAT.
pub const V2_FIX: &str =
    "Topo-id windows made f=2d vacuously UNSAT; publish times + an f-sweep are the right toy check";

/// Under publish-time encoding, stated f=2d has a cex except one small instance.
pub const V2_CEX: &str = "Under that encoding, f=2d has a cex for every instance except n=6 d=2";

/// Finite “safe” f tracks graph size more than delay.
pub const V2_TAKEAWAY: &str =
    "The finite safe f is about n-2, so it tracks graph size more than delay — not a KIP-2 invariant";

/// Task A* nontrivial blue sets.
pub const V2_TASK_A_BLUES: &str = "Task A* blues: n=6 [0,3,4], n=8 [0,1,2,5]";

/// Round 3: independent solvers + checked SAT models. Still not a protocol proof.
pub const ROUND3_NOTE: &str =
    "Round 3 verified: Glucose4+Cadical195+Minisat22 agree; SAT models clause-checked; still not a KIP-2 verdict";

/// host02 native kissat + drat-trim. Checked DRAT is a CNF proof, not KIP-2.
pub const HOST02_PROOF: &str =
    "host02 kissat+drat-trim: boundary UNSAT CNFs have verified DRAT; still not a KIP-2 verdict";

/// tuce/flywheel/engos/mamajama stay on host02 and stay off unless a new UNSAT needs native DRAT.
pub const HOST02_WHEN: &str =
    "host02 tuce/flywheel/engos/mamajama: not needed for current KIP-2 toys; only kissat+drat-trim if a new UNSAT CNF cannot be DRAT-checked on Windows PySAT. Do not add them as crate deps. Do not resume satcomp grind";

/// A KIP-2 verdict is a protocol event. This crate cannot mint one.
pub const HOW_A_VERDICT: &str = "KIP-2 verdict = kaspanet/kips Status leaves Proposed, DK in rusty-kaspa, public testnet, then mainnet HF. SAT/DRAT here cannot ratify it";

/// Printed by `tn10-status` — logged discussion status, not activation.
pub fn print_round1() {
    println!("KIP-2 SAT (logged, not ratified; not mainnet):");
    println!("  {V2_FIX}");
    println!("  {V2_CEX}");
    println!("  {V2_TAKEAWAY}");
    println!("  {V2_TASK_A_BLUES}");
    println!("  {ROUND3_NOTE}");
    println!("  {HOST02_PROOF}");
    println!("  {HOST02_WHEN}");
    println!("  {HOW_A_VERDICT}");
    println!("  Send to the KIP thread, not mainnet.");
    println!("  artifacts: AI Agent/scripts/dagknight_sat/out_v2/VERIFIED.md");
    println!("  still not a KIP-2 verdict (live L1 remains GHOSTDAG @ 10 BPS)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logged_not_ratified() {
        assert!(V2_CEX.contains("except n=6 d=2"));
        assert!(V2_TAKEAWAY.contains("not a KIP-2 invariant"));
        assert!(V2_TASK_A_BLUES.contains("[0,3,4]"));
        assert!(V2_TASK_A_BLUES.contains("[0,1,2,5]"));
        assert!(ROUND3_NOTE.contains("Glucose4+Cadical195+Minisat22"));
        assert!(ROUND3_NOTE.contains("still not a KIP-2 verdict"));
        assert!(HOST02_PROOF.contains("verified DRAT"));
        assert!(HOST02_PROOF.contains("not a KIP-2 verdict"));
        assert!(HOST02_WHEN.contains("not needed for current KIP-2"));
        assert!(HOST02_WHEN.contains("Do not add them as crate deps"));
        assert!(HOW_A_VERDICT.contains("Status leaves Proposed"));
        assert!(HOW_A_VERDICT.contains("SAT/DRAT here cannot ratify it"));
        assert!(!V2_TAKEAWAY.to_ascii_lowercase().contains("kip-2 is wrong"));
        assert!(!V2_CEX.to_ascii_lowercase().contains("activated"));
    }
}
