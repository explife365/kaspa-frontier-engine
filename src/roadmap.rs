//! Local, source-backed Kaspa roadmap snapshot as of 25 Aug 2026. Work belongs on
//! TN10 clients and wallets — not in invented consensus patches.
//!
//! Live: GHOSTDAG @ 10 BPS, Toccata (KIP-16/17/20/21).
//! Experimental: SilverScript / Argent.
//! Draft: KCC-0020 covenant tokens.
//! Research: DAGKnight (KIP-2 Proposed). Not activated.
//! Off L1: Igra/Kasplex EVM, Kasplex KRC-20 indexer. Not kaspad consensus.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackStatus {
    Live,
    Experimental,
    Draft,
    Research,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoadmapTrack {
    pub name: &'static str,
    pub status: TrackStatus,
    pub note: &'static str,
}

pub const GHOSTDAG_10BPS: RoadmapTrack = RoadmapTrack {
    name: "GHOSTDAG @ 10 BPS",
    status: TrackStatus::Live,
    note: "Current mainnet and TN10 consensus. Not DAGKnight.",
};

pub const TOCCATA: RoadmapTrack = RoadmapTrack {
    name: "Toccata (KIP-16/17/20/21)",
    status: TrackStatus::Live,
    note: "L1 covenants, covenant IDs, OpZkPrecompile. No EVM on L1.",
};

pub const SILVERSCRIPT: RoadmapTrack = RoadmapTrack {
    name: "SilverScript / Argent",
    status: TrackStatus::Experimental,
    note: "TN10 compiler + actor language. Unaudited. Proof = public txid.",
};

pub const KCC_0020: RoadmapTrack = RoadmapTrack {
    name: "KCC-0020",
    status: TrackStatus::Draft,
    note: "Covenant token convention. Distinct from Kasplex KRC-20.",
};

pub const DAGKNIGHT: RoadmapTrack = RoadmapTrack {
    name: "DAGKnight (KIP-2)",
    status: TrackStatus::Research,
    note: "Proposed. Do not emit 100 BPS telemetry as if live.",
};

pub const PROTOCOL_LABEL: &str = "GHOSTDAG @ 10 BPS (Crescendo). DAGKnight is not activated.";

pub const TRACKS: &[RoadmapTrack] = &[GHOSTDAG_10BPS, TOCCATA, SILVERSCRIPT, KCC_0020, DAGKNIGHT];

/// Why a community DeFi/listing ask cannot be done *inside kaspad*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct L1Gap {
    pub ask: &'static str,
    pub why_not_l1: &'static str,
    pub belongs: &'static str,
}

pub const L1_GAPS: &[L1Gap] = &[
    L1Gap {
        ask: "Solidity / ERC-20 / Uniswap",
        why_not_l1: "kaspad is UTXO + Toccata covenants; no EVM runtime",
        belongs: "Igra Galleon / Kasplex L2",
    },
    L1Gap {
        ask: "Circle USDC",
        why_not_l1: "issuer product; L1 cannot host ERC-20",
        belongs: "Circle + Igra/Kasplex production (Galleon test USDC and Igra Hyperlane USDC are not Circle)",
    },
    L1Gap {
        ask: "getUtxosByCovenantId",
        why_not_l1: "missing from kaspad RPC",
        belongs: "kascov community indexer until a KIP adds it",
    },
    L1Gap {
        ask: "Native covenant tokens",
        why_not_l1: "KCC-0020 is draft, not consensus",
        belongs: "wait for ratification; not Kasplex KRC-20",
    },
    L1Gap {
        ask: "DAGKnight / 100 BPS lore",
        why_not_l1: "KIP-2 Proposed; GHOSTDAG @ 10 BPS is what runs. Fake telemetry does not activate it",
        belongs: "research / narrative only",
    },
    L1Gap {
        ask: "Covenant UX lag",
        why_not_l1: "Toccata is live (~517 mainnet covenants vs ~80k TN10). kaspad already accepts v1 txs; wallets/explorers lag",
        belongs: "wallet/explorer vendors + kascov/Covex, not a consensus patch",
    },
    L1Gap {
        ask: "Archival / indexer cost",
        why_not_l1: "exchanges need getUtxosByAddresses + DAA depth, not a simulated worker. Archive cost is ops, not missing consensus",
        belongs: "kaspad REST /addresses/{}/utxos + this crate’s deposit/withdraw DAA tracker",
    },
    L1Gap {
        ask: "Binance/Coinbase spot",
        why_not_l1: "listing is exchange custody + demand, not a node patch",
        belongs: "the CEX; this crate is an integrator rehearsal",
    },
];

pub fn is_activated(track: &RoadmapTrack) -> bool {
    matches!(track.status, TrackStatus::Live)
}

pub fn print_tracks() {
    println!("roadmap            {PROTOCOL_LABEL}");
    for track in TRACKS {
        let tag = match track.status {
            TrackStatus::Live => "live",
            TrackStatus::Experimental => "experimental",
            TrackStatus::Draft => "draft",
            TrackStatus::Research => "research",
        };
        println!("  [{tag}] {} — {}", track.name, track.note);
    }
}

pub fn print_l1_gaps() {
    println!("what is blocking kaspad (not this crate):");
    println!("  kaspad is UTXO + GHOSTDAG @ 10 BPS + Toccata. No EVM. DAGKnight is not live.");
    for gap in L1_GAPS {
        println!("  {} — {} → {}", gap.ask, gap.why_not_l1, gap.belongs);
    }
}

/// Work this crate still owns. Not consensus.
pub fn print_integrator_next() {
    println!("integrator next:");
    println!(
        "  gTEST is live on Galleon {}; not USD; not Circle USDC",
        crate::network::GALLEON_GTEST
    );
    if let Some(wikas) = crate::network::GALLEON_WRAPPED_IKAS {
        println!("  wiKAS is live on Galleon {wikas}; not kaspad; not USD");
    }
    println!("  Use Igra Galleon test USDC; do not deploy a USDC lookalike");
    println!(
        "  Igra mainnet Hyperlane USDC {} is bridged HypSynthetic, not Circle",
        crate::network::IGRA_MAINNET_HYPERLANE_USDC
    );
    println!("  Test gas: respect Igra faucet limits; do not automate IP rotation or multi-account bypasses");
    println!("  L1 testnet is TN10 only; do not IBD TN12. Rothschild -t is TPS (tx/s), not BPS");
    println!(
        "  100 BPS is KIP-2 lore; scripts/bps100_cnf.py vs GHOSTDAG k=18. TARGET_BPS stays 10"
    );
    println!("  DRAT tooling: off unless a new UNSAT CNF lacks a verified proof");
    println!("  EVM work is Galleon L2 (wiKAS live); kaspad has no EVM — do not add one");
    println!("  CEX rehearsal: bounded multi-address ingestion + durable exact withdrawals + scheduled/dead-letter webhook outbox + atomic deduplicating receiver + delta-driven durable wRPC + subscribe-then-REST-scan (rusty-kaspa#939) + subscribe-ack journal replay applies to ledger on restart + ordered owned-node failover that will not subscribe to an unhealthy replica + shared N-of-M health gate (--dual, scripts/tn10_gate.ps1) on health/deposits/withdraw/outbox/receiver + mTLS required for non-loopback webhooks. Production still needs independently hosted nodes");
    println!("  SDK gate: python scripts/tn10_sdk_gate.py --json (native covenant broadcast fail-closed until kaspa-python-sdk#78; TN10 dev patch for rehearsal)");
    println!("  CertiK Skynet gap (~84.6 vs BTC ~97.5) is Foundation telemetry/ops — see scripts/certik_score_plan.md; maps to COMMUNITY_ASKS, not KIP-2 or BPS lore");
}

/// Integrator asks this crate can ship, park on L2, or refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskStatus {
    Shipped,
    Partial,
    OnL2,
    Refused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommunityAsk {
    pub ask: &'static str,
    pub status: AskStatus,
    pub note: &'static str,
}

pub const COMMUNITY_ASKS: &[CommunityAsk] = &[
    CommunityAsk {
        ask: "TN10 deposits with DAA",
        status: AskStatus::Shipped,
        note: "tn10-deposits + credited ledger (no double-credit on restart)",
    },
    CommunityAsk {
        ask: "Withdraw dest UTXO + DAA",
        status: AskStatus::Shipped,
        note: "tn10-withdraw checks dest address on REST UTXOs (DAG+UTXOs, no unused balance GET)",
    },
    CommunityAsk {
        ask: "Toccata v1 parse + fee floor",
        status: AskStatus::Shipped,
        note: "tn10-proof + 100 sompi/gram floor; tn10-status flags buckets below floor",
    },
    CommunityAsk {
        ask: "getUtxosByCovenantId",
        status: AskStatus::Partial,
        note: "kascov covenant documents with typed embedded UTXOs + bounded tn10-covenant-rpc shim + tn10-proof REST/kascov/--kascov-only verification; reference apps counter + timelock_vault + restricted_swap with public TN10 proof fixtures (native broadcast fail-closed until kaspa-python-sdk#78; TN10 dev patch for rehearsal); scripts/tn10_covenant_rehearsal.ps1 + tn10_covenant_rpc_smoke.ps1. still not kaspad",
    },
    CommunityAsk {
        ask: "Kasplex KRC-20 mint/transfer",
        status: AskStatus::Shipped,
        note: "TMBMN live; compact mint uses deploy lim; Python indexer GETs reuse TN10 TLS keep-alive",
    },
    CommunityAsk {
        ask: "gTEST ERC-20 on Galleon",
        status: AskStatus::Shipped,
        note: "0xbc5e27ab3ce2edb243593cda2437e5b30e0d5d7d permit ERC-20; not USD; not Circle",
    },
    CommunityAsk {
        ask: "Circle USDC / Uniswap on L1",
        status: AskStatus::OnL2,
        note: "Galleon test USDC 0xFd89…667A via JSON-RPC batch eth_call; Igra mainnet Hyperlane USDC 0xA5b8…735E7 is bridged, not Circle; L1 has no EVM",
    },
    CommunityAsk {
        ask: "DAGKnight / 100 BPS lore",
        status: AskStatus::Refused,
        note: "narrative only. KIP-2 Proposed; live is GHOSTDAG @ 10 BPS. PDF3 dagknight.rs / 100 BPS fork refused. Local SAT fragments are research evidence only, not activation or a KIP verdict",
    },
    CommunityAsk {
        ask: "Covenant UX lag",
        status: AskStatus::Partial,
        note: "Toccata live (~517 mainnet vs ~80k TN10). We print lineage+explorer; wallets/explorers still catching up",
    },
    CommunityAsk {
        ask: "Archival / indexer (getUtxosByAddresses + DAA)",
        status: AskStatus::Shipped,
        note: "REST /addresses/{}/utxos + deposit DAA depth + restart-safe SQLite withdrawal observations. Not a simulated worker",
    },
    CommunityAsk {
        ask: "CEX plug-and-play REST wrapper",
        status: AskStatus::Partial,
        note: "rehearsal only: bounded multi-address snapshots/ingestion, durable exact withdrawals, scheduled/dead-letter webhook outbox, atomic deduplicating receiver inbox, delta-driven durable wRPC replay, subscribe-then-REST-scan (rusty-kaspa#939), ordered owned-node failover that will not subscribe to an unhealthy replica, N-of-M supervisor health gate, and mTLS required for non-loopback webhook delivery. Production still needs independently hosted nodes",
    },
    CommunityAsk {
        ask: "Kasplex tokenlist pagination",
        status: AskStatus::Shipped,
        note: "next cursor percent-encoded (tn10-kasplex); info+tokenlist concurrent; address/tick paths join indexer info",
    },
    CommunityAsk {
        ask: "Local kaspad TN10 IBD",
        status: AskStatus::Partial,
        note: "owned kaspad 2.0.1 runs --utxoindex on the tn10 appdir; health fails closed on UTXO import (DAA 0), IBD peers, header/body gap, isolation, lag, and N-of-M. Node 1 and node 2 synced; --min-healthy 2 green when both loopback endpoints are up. Toccata broadcast fail-closed until kaspa-python-sdk #78 wheel",
    },
    CommunityAsk {
        ask: "KIP-2 SAT fragments",
        status: AskStatus::Partial,
        note: "v2 re-solved locally; native kissat DRAT verified on reference CNFs. Extra grind tooling not needed unless a new UNSAT lacks DRAT. Not a KIP verdict",
    },
];

pub fn community_ask_tally() -> (u32, u32, u32, u32) {
    let mut shipped = 0u32;
    let mut partial = 0u32;
    let mut l2 = 0u32;
    let mut refused = 0u32;
    for item in COMMUNITY_ASKS {
        match item.status {
            AskStatus::Shipped => shipped += 1,
            AskStatus::Partial => partial += 1,
            AskStatus::OnL2 => l2 += 1,
            AskStatus::Refused => refused += 1,
        }
    }
    (shipped, partial, l2, refused)
}

pub fn print_community_asks() {
    println!("community asks (this crate):");
    for item in COMMUNITY_ASKS {
        let tag = match item.status {
            AskStatus::Shipped => "shipped",
            AskStatus::Partial => "partial",
            AskStatus::OnL2 => "l2",
            AskStatus::Refused => "refused",
        };
        println!("  [{tag}] {} — {}", item.ask, item.note);
    }
    let (shipped, partial, l2, refused) = community_ask_tally();
    println!("  tally shipped={shipped} partial={partial} l2={l2} refused={refused}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dagknight_is_not_live() {
        assert!(!is_activated(&DAGKNIGHT));
        assert!(is_activated(&TOCCATA));
        assert!(is_activated(&GHOSTDAG_10BPS));
        assert_eq!(SILVERSCRIPT.status, TrackStatus::Experimental);
        assert!(!PROTOCOL_LABEL.contains("activated DAGKnight"));
        assert!(L1_GAPS
            .iter()
            .any(|g| g.ask.contains("getUtxosByCovenantId")));
        assert!(TOCCATA.note.contains("No EVM"));
        assert_eq!(L1_GAPS.len(), 8);
        assert!(L1_GAPS.iter().any(|g| g.ask.contains("Covenant UX")));
        assert!(L1_GAPS.iter().any(|g| g.ask.contains("Archival")));
        assert!(L1_GAPS.iter().any(|g| g.ask.contains("USDC")));
        assert!(L1_GAPS.iter().all(|g| !g.why_not_l1.is_empty()));
    }

    #[test]
    fn integrator_next_is_not_consensus() {
        assert!(PROTOCOL_LABEL.contains("GHOSTDAG"));
        assert!(!PROTOCOL_LABEL
            .to_ascii_lowercase()
            .contains("activated dagknight"));
        assert!(COMMUNITY_ASKS
            .iter()
            .any(|a| a.status == AskStatus::Shipped));
        assert!(COMMUNITY_ASKS
            .iter()
            .any(|a| a.status == AskStatus::Refused));
        assert!(COMMUNITY_ASKS.iter().any(|a| a.ask.contains("gTEST")));
        assert_eq!(COMMUNITY_ASKS.len(), 14);
        let (shipped, partial, l2, refused) = community_ask_tally();
        assert_eq!(shipped + partial + l2 + refused, 14);
        assert!(shipped >= 5);
        assert!(COMMUNITY_ASKS
            .iter()
            .any(|a| a.ask.contains("CEX") && a.status == AskStatus::Partial));
        assert_eq!(refused, 1);
        assert!(COMMUNITY_ASKS.iter().any(|a| a.ask.contains("Covenant UX")));
        assert!(COMMUNITY_ASKS
            .iter()
            .any(|a| a.ask.contains("getUtxosByAddresses")));
        assert!(COMMUNITY_ASKS.iter().any(|a| a.ask.contains("CEX")));
    }
}
