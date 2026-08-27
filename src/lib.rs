//! Honest Kaspa integration crate.
//!
//! Gemini mixed strategy with simulated engines, fake rusty-kaspa patches,
//! and invented Criterion numbers. This crate keeps the useful pieces:
//! TN10 connectivity, deposit DAA confirmation, KRC-20 off-chain indexing,
//! and GHOSTDAG telemetry from live DAG info.
//!
//! Independent dev sig / optional mainnet KAS:
//! `kaspa:qpxdemlyx445kt5xteux0qhadaw8lh5m0vnqvcy8fh483t70usgkkeulsx9cm`

pub mod cex;
pub mod circle;
pub mod covenant;
pub mod covenant_rpc;
pub mod deposit_ledger;
pub mod erc20;
pub mod error;
pub mod exchange;
pub mod galleon;
pub mod kascov;
pub mod kasplex;
pub mod kip2_sat;
pub mod krc20;
pub mod l2;
pub mod mtls;
pub mod network;
pub mod outbox_receiver;
pub mod owned_node;
pub mod proof;
pub mod rest;
pub mod roadmap;
pub mod rpc;
pub mod telemetry;
pub mod watch;
pub mod withdrawal_ledger;
pub mod wrpc;

pub use cex::{
    fetch_address_snapshots, snapshot_address, snapshot_address_default, CexAddressSnapshot,
    CexSpendable, OutpointSpendGuard,
};
pub use covenant::{CovenantError, CovenantPolicyEngine, NativeCovenantUtxo};
pub use deposit_ledger::{
    ClaimedLedgerEvent, DeliveryFailureOutcome, DepositLedger, LedgerEvent, OutboxEventStatus,
};
pub use erc20::Erc20Meta;
pub use error::{EngineError, Result};
pub use exchange::{
    confirm_withdrawal, ConfirmedDeposit, ConfirmedWithdrawal, DepositTracker, PendingDeposit,
    UtxoAppearance, WithdrawalExpectation, WithdrawalUtxo,
};
pub use galleon::{entry_payload, parse_l2_address};
pub use kascov::{KascovClient, KascovCoin, KascovEvent, KascovStateField, KascovUtxo};
pub use kasplex::{
    KasplexBalance, KasplexClient, KasplexInfo, KasplexOp, KasplexStatus, KasplexToken,
    KasplexTokenPage,
};
pub use krc20::{
    kasplex_deploy_inscription, kasplex_mint_inscription, kasplex_tick,
    kasplex_transfer_inscription, Krc20Error, Krc20StateEngine,
};
pub use l2::{EvmChainProbe, EvmRpcClient};
pub use network::AddressNetwork;
pub use outbox_receiver::{
    DeliveryEnvelope, InboxOutcome, IncomingLedgerEvent, OutboxReceiverStore,
};
pub use owned_node::{
    assess_owned_node, choose_failover_index, next_failover_index, probe_owned_node,
    require_loopback_wrpc_url, select_primary, validate_owned_node_urls, OwnedNodeAssessment,
    OwnedNodeHealth, MAX_OWNED_NODE_URLS,
};
pub use proof::{CovenantProof, CovenantProofStep};
pub use rest::{
    AddressBalance, AddressUtxo, BlockDagInfo, FeeEstimate, HashrateInfo, StatusSnapshot,
    Tn10RestClient, ToccataTx,
};
pub use roadmap::{
    community_ask_tally, is_activated, AskStatus, CommunityAsk, L1Gap, RoadmapTrack, TrackStatus,
    COMMUNITY_ASKS, L1_GAPS, PROTOCOL_LABEL, TRACKS,
};
pub use telemetry::GhostdagTelemetry;
pub use watch::{
    poll_durable_withdrawal, poll_withdrawal, withdrawal_utxos, DepositWatch, WatchTick,
};
pub use withdrawal_ledger::{WithdrawalLedger, WithdrawalRecord, WithdrawalState};
pub use wrpc::{
    decode_block_dag_info_response, decode_notification, decode_server_info_response,
    encode_get_block_dag_info, encode_get_server_info, encode_notify_utxos_changed,
    encode_notify_virtual_daa_score_changed, replay_into_ledger, replay_into_ledger_addresses,
    validate_subscription_ack, WrpcBlockDagInfo, WrpcDepositProjection, WrpcDepositSnapshot,
    WrpcFrame, WrpcJournal, WrpcNotification, WrpcReplayReport, WrpcServerInfo,
};
