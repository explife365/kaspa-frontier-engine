//! Owned-node wRPC JSON ingestion for TN10 deposits.
//! Read-only: subscribes to UTXO/DAA notifications and writes only local SQLite state.

use futures_util::{SinkExt, StreamExt};
use kaspa_frontier_engine::network::{
    is_valid_testnet_address, require_tn10, TESTNET_10_REST,
};
use kaspa_frontier_engine::{
    apply_pending_journal_frames, assess_owned_node, decode_notification,
    encode_notify_utxos_changed, encode_notify_virtual_daa_score_changed, finish_gate_options,
    owned_node_stage, owned_node_stage_label, print_gate_preflight, probe_owned_node,
    run_owned_node_gate, select_primary, try_parse_gate_flag, validate_subscription_ack, AddressUtxo, DepositLedger, OwnedNodeAssessment,
    OwnedNodeGateOptions, Tn10RestClient, WrpcDepositProjection, WrpcJournal, WrpcReplayReport,
    GATE_DEFAULT_MAX_DAA_LAG,
};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MESSAGE_TIMEOUT: Duration = Duration::from_secs(30);
const RESNAPSHOT_INTERVAL: Duration = Duration::from_secs(60);

struct Options {
    addresses: Vec<String>,
    gate: OwnedNodeGateOptions,
    rest: String,
    database: PathBuf,
    resnapshot_only: bool,
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut addresses = Vec::new();
    let mut gate = OwnedNodeGateOptions {
        max_daa_lag: GATE_DEFAULT_MAX_DAA_LAG,
        ..OwnedNodeGateOptions::default()
    };
    let mut rest = TESTNET_10_REST.to_string();
    let mut database = PathBuf::from(".local/tn10-wrpc-live.sqlite");
    let mut resnapshot_only = false;
    let mut dual = false;
    let mut args = arguments.into_iter();
    while let Some(argument) = args.next() {
        if try_parse_gate_flag(&mut gate, &mut dual, &argument, &mut args)? {
            continue;
        }
        match argument.as_str() {
            "--rest" => rest = args.next().ok_or("--rest needs a value")?,
            "--database" => database = PathBuf::from(args.next().ok_or("--database needs a path")?),
            "--resnapshot-only" => resnapshot_only = true,
            _ if argument.starts_with('-') => return Err(format!("unknown flag {argument}")),
            _ => addresses.push(argument),
        }
    }
    if addresses.is_empty() || addresses.len() > kaspa_frontier_engine::rest::MAX_ADDRESS_BATCH {
        return Err(format!(
            "tn10-wrpc-live requires 1-{} addresses",
            kaspa_frontier_engine::rest::MAX_ADDRESS_BATCH
        ));
    }
    let mut unique = HashSet::with_capacity(addresses.len());
    for address in &addresses {
        if !is_valid_testnet_address(address) {
            return Err(format!("{address} must be a checksummed kaspatest address"));
        }
        if !unique.insert(address) {
            return Err(format!("duplicate watched address {address}"));
        }
    }
    let gate = finish_gate_options(gate, dual)?;
    Ok(Options {
        addresses,
        gate,
        rest,
        database,
        resnapshot_only,
    })
}

fn usage() -> &'static str {
    "usage: tn10-wrpc-live ADDRESS [ADDRESS ...] [--url ws://127.0.0.1:18210]... [--dual] [--min-healthy N] [--require-healthy N] [--rest HTTPS] [--database PATH] [--max-daa-lag 100] [--resnapshot-only]"
}

async fn fetch_utxo_snapshot(
    rest: &Tn10RestClient,
    addresses: &[String],
) -> kaspa_frontier_engine::Result<(u64, Vec<AddressUtxo>)> {
    let (dag, utxos) = tokio::join!(rest.block_dag_info(), rest.utxos_for_addresses(addresses));
    let dag = dag?;
    require_tn10(&dag.network_name)?;
    Ok((dag.virtual_daa_score, utxos?))
}

fn apply_fetched_snapshot(
    virtual_daa: u64,
    utxos: &[AddressUtxo],
    addresses: &[String],
    projection: &mut WrpcDepositProjection,
    ledger: &mut DepositLedger,
) -> kaspa_frontier_engine::Result<()> {
    let mut snapshot = projection.bootstrap_addresses(virtual_daa, utxos, addresses)?;
    let current: HashSet<_> = snapshot
        .observed
        .iter()
        .map(|entry| (entry.tx_id.clone(), entry.output_index))
        .collect();
    for pending in ledger.pending_outpoints_for_addresses(addresses)? {
        if !current.contains(&pending) && !snapshot.disappeared_outpoints.contains(&pending) {
            snapshot.disappeared_outpoints.push(pending);
        }
    }
    ledger.reconcile(
        snapshot.virtual_daa,
        &snapshot.observed,
        &snapshot.confirmed,
        &snapshot.disappeared_outpoints,
    )
}

async fn resnapshot(
    rest: &Tn10RestClient,
    addresses: &[String],
    projection: &mut WrpcDepositProjection,
    ledger: &mut DepositLedger,
) -> kaspa_frontier_engine::Result<()> {
    let (virtual_daa, utxos) = fetch_utxo_snapshot(rest, addresses).await?;
    apply_fetched_snapshot(virtual_daa, &utxos, addresses, projection, ledger)
}

/// rusty-kaspa#939: kaspad cannot filter UtxosChanged by min DAA. Subscribe first,
/// REST-scan while buffering unread frames, then apply the buffer (not skip it).
async fn scan_while_subscribed(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    rest: &Tn10RestClient,
    addresses: &[String],
    journal: &mut WrpcJournal,
    ledger: &mut DepositLedger,
    projection: &mut WrpcDepositProjection,
    source: &str,
) -> Result<WrpcReplayReport, Box<dyn std::error::Error>> {
    let fetch = fetch_utxo_snapshot(rest, addresses);
    tokio::pin!(fetch);
    let (virtual_daa, utxos) = loop {
        tokio::select! {
            result = &mut fetch => break result?,
            raw = receive_text(socket) => {
                journal.append(source, &raw?)?;
            }
        }
    };
    apply_fetched_snapshot(virtual_daa, &utxos, addresses, projection, ledger)?;
    Ok(apply_pending_journal_frames(
        journal,
        ledger,
        projection,
        source,
        addresses,
    )?)
}

fn replay_pending_after_resnapshot(
    journal: &mut WrpcJournal,
    ledger: &mut DepositLedger,
    projection: &mut WrpcDepositProjection,
    source: &str,
    addresses: &[String],
) -> kaspa_frontier_engine::Result<WrpcReplayReport> {
    apply_pending_journal_frames(journal, ledger, projection, source, addresses)
}

fn apply_live_frame(
    raw: &str,
    journal: &mut WrpcJournal,
    ledger: &mut DepositLedger,
    projection: &mut WrpcDepositProjection,
    source: &str,
    addresses: &[String],
) -> kaspa_frontier_engine::Result<()> {
    let notification = decode_notification(raw)?;
    if let Some(snapshot) = projection.apply_addresses(notification, addresses)? {
        let sequence = journal.append(source, raw)?;
        ledger.reconcile(
            snapshot.virtual_daa,
            &snapshot.observed,
            &snapshot.confirmed,
            &snapshot.disappeared_outpoints,
        )?;
        journal.mark_applied(source, sequence)?;
        if sequence % 1_000 == 0 {
            let _ = journal.prune_applied(source, 100)?;
        }
    } else {
        let sequence = journal.append_applied(source, raw)?;
        if sequence % 1_000 == 0 {
            let _ = journal.prune_applied(source, 100)?;
        }
    }
    Ok(())
}

async fn receive_text(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Result<String, Box<dyn std::error::Error>> {
    loop {
        let message = tokio::time::timeout(MESSAGE_TIMEOUT, socket.next())
            .await
            .map_err(|_| "wRPC message timeout")?
            .ok_or("wRPC connection closed")??;
        match message {
            Message::Text(text) => return Ok(text.to_string()),
            Message::Ping(payload) => socket.send(Message::Pong(payload)).await?,
            Message::Pong(_) => {}
            Message::Close(frame) => return Err(format!("wRPC close frame: {frame:?}").into()),
            Message::Binary(_) => return Err("wRPC JSON endpoint sent a binary frame".into()),
            Message::Frame(_) => {}
        }
    }
}

async fn run_connection(
    options: &Options,
    url: &str,
    rest: &Tn10RestClient,
    journal: &mut WrpcJournal,
    ledger: &mut DepositLedger,
    projection: &mut WrpcDepositProjection,
    source: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(kaspa_frontier_engine::wrpc::MAX_WRPC_FRAME_BYTES);
    config.max_frame_size = Some(kaspa_frontier_engine::wrpc::MAX_WRPC_FRAME_BYTES);
    let (mut socket, _) = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio_tungstenite::connect_async_with_config(url, Some(config), false),
    )
    .await
    .map_err(|_| "wRPC connect timeout")??;

    let utxo_request = encode_notify_utxos_changed(1, &options.addresses)?;
    let daa_request = encode_notify_virtual_daa_score_changed(2)?;
    socket.send(Message::Text(utxo_request.into())).await?;
    socket.send(Message::Text(daa_request.into())).await?;

    let mut utxo_ack = false;
    let mut daa_ack = false;
    while !utxo_ack || !daa_ack {
        let raw = receive_text(&mut socket).await?;
        let value: serde_json::Value = serde_json::from_str(&raw)?;
        match value.get("id").and_then(serde_json::Value::as_u64) {
            Some(1) => {
                validate_subscription_ack(&raw, 1, "subscribe")?;
                utxo_ack = true;
            }
            Some(2) => {
                validate_subscription_ack(&raw, 2, "subscribe")?;
                daa_ack = true;
            }
            Some(id) => return Err(format!("unexpected wRPC response id {id}").into()),
            None => {
                journal.append(source, &raw)?;
            }
        }
    }
    println!(
        "subscribed  {} addresses + virtual DAA via {}",
        options.addresses.len(),
        url
    );
    let replay = scan_while_subscribed(
        &mut socket,
        rest,
        &options.addresses,
        journal,
        ledger,
        projection,
        source,
    )
    .await?;
    println!(
        "resnapshot  live={} checkpoint={}",
        replay.live_utxos, replay.checkpoint
    );

    let mut resnapshot_interval = tokio::time::interval(RESNAPSHOT_INTERVAL);
    resnapshot_interval.tick().await;
    loop {
        tokio::select! {
            _ = resnapshot_interval.tick() => {
                let replay = scan_while_subscribed(
                    &mut socket,
                    rest,
                    &options.addresses,
                    journal,
                    ledger,
                    projection,
                    source,
                )
                .await?;
                println!(
                    "resnapshot  live={} checkpoint={}",
                    replay.live_utxos, replay.checkpoint
                );
            }
            message = tokio::time::timeout(MESSAGE_TIMEOUT, socket.next()) => {
                let message = message
                    .map_err(|_| "wRPC message timeout")?
                    .ok_or("wRPC connection closed")??;
                match message {
                    Message::Text(text) => apply_live_frame(
                        text.as_ref(),
                        journal,
                        ledger,
                        projection,
                        source,
                        &options.addresses,
                    )?,
                    Message::Ping(payload) => socket.send(Message::Pong(payload)).await?,
                    Message::Pong(_) => {}
                    Message::Close(frame) => {
                        return Err(format!("wRPC close frame: {frame:?}").into());
                    }
                    Message::Binary(_) => {
                        return Err("wRPC JSON endpoint sent a binary frame".into());
                    }
                    Message::Frame(_) => {}
                }
            }
        }
    }
}

fn source_for_addresses(addresses: &[String]) -> String {
    let mut canonical = addresses.to_vec();
    canonical.sort();
    let mut hash = 0xcbf29ce484222325u64;
    for address in &canonical {
        for byte in address.bytes().chain(std::iter::once(0)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("owned-node:{}:{hash:016x}", canonical.len())
}

async fn healthy_replicas(
    urls: &[String],
    rest: &Tn10RestClient,
    max_daa_lag: u64,
    skip: Option<usize>,
) -> Result<Vec<(usize, OwnedNodeAssessment)>, Box<dyn std::error::Error>> {
    let public = rest.block_dag_info().await?;
    let probes = futures_util::future::join_all(urls.iter().map(|url| probe_owned_node(url))).await;
    let mut healthy = Vec::new();
    for (index, probe) in probes.into_iter().enumerate() {
        if skip == Some(index) {
            continue;
        }
        if let Ok(health) = probe {
            if let Ok(assessment) =
                assess_owned_node(&health, public.virtual_daa_score, max_daa_lag)
            {
                healthy.push((index, assessment));
            }
        }
    }
    Ok(healthy)
}

/// Prefer a healthy replica. If none remain, hold the current index — do not
/// subscribe to an unsynced or lagged node.
fn hold_or_select_healthy(
    current_index: usize,
    healthy: &[(usize, OwnedNodeAssessment)],
) -> (usize, bool) {
    match select_primary(healthy) {
        Some(index) => (index, true),
        None => (current_index, false),
    }
}

async fn print_selected_node(url: &str) {
    if let Ok(health) = probe_owned_node(url).await {
        let stage = owned_node_stage(&health);
        println!("selected    {} [{}]", url, owned_node_stage_label(stage));
    }
}

async fn next_owned_node(
    urls: &[String],
    rest: &Tn10RestClient,
    failed_index: usize,
    max_daa_lag: u64,
) -> Result<(usize, bool), Box<dyn std::error::Error>> {
    let healthy = healthy_replicas(urls, rest, max_daa_lag, Some(failed_index)).await?;
    Ok(hold_or_select_healthy(failed_index, &healthy))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options =
        parse_args(env::args().skip(1)).map_err(|error| format!("{error}\n{}", usage()))?;
    if let Some(parent) = options.database.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let rest = Tn10RestClient::new(&options.rest)?;
    let source = source_for_addresses(&options.addresses);
    let mut journal = WrpcJournal::open(&options.database)?;
    let mut ledger = DepositLedger::open(&options.database)?;
    let mut projection = WrpcDepositProjection::default();

    resnapshot(&rest, &options.addresses, &mut projection, &mut ledger).await?;
    let replay = replay_pending_after_resnapshot(
        &mut journal,
        &mut ledger,
        &mut projection,
        &source,
        &options.addresses,
    )?;
    println!("database    {}", options.database.display());
    println!("addresses   {}", options.addresses.len());
    println!("owned nodes {}", options.gate.urls.len());
    println!(
        "resnapshot  live={} checkpoint={}",
        replay.live_utxos, replay.checkpoint
    );
    println!(
        "outbox      pending={} (deliver with tn10-outbox; never auto-acknowledged)",
        ledger.unacknowledged_count()?
    );
    if options.resnapshot_only {
        return Ok(());
    }
    if options.gate.min_healthy > 0 {
        let summary = run_owned_node_gate(
            &options.gate.urls,
            &rest,
            options.gate.max_daa_lag,
            options.gate.min_healthy,
        )
        .await?;
        print_gate_preflight(&summary);
    }

    let mut delay = Duration::from_secs(1);
    let mut node_index = loop {
        match healthy_replicas(
            &options.gate.urls,
            &rest,
            options.gate.max_daa_lag,
            None,
        )
        .await
        {
            Ok(healthy) => match select_primary(&healthy) {
                Some(index) => {
                    delay = Duration::from_secs(1);
                    print_selected_node(&options.gate.urls[index]).await;
                    break index;
                }
                None => {
                    eprintln!(
                        "owned-node health: no replica within {} DAA; not subscribing",
                        options.gate.max_daa_lag
                    );
                }
            },
            Err(error) => eprintln!("owned-node health probe retry required: {error}"),
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(30));
    };
    loop {
        let url = options.gate.urls[node_index].clone();
        println!("connecting  {url}");
        match run_connection(
            &options,
            &url,
            &rest,
            &mut journal,
            &mut ledger,
            &mut projection,
            &source,
        )
        .await
        {
            Ok(()) => unreachable!("wRPC connection loop only returns on failure"),
            Err(error) => eprintln!("wRPC failover from {url}: {error}"),
        }
        let (next_index, found_healthy) =
            match next_owned_node(
                &options.gate.urls,
                &rest,
                node_index,
                options.gate.max_daa_lag,
            )
            .await
            {
                Ok(choice) => choice,
                Err(error) => {
                    eprintln!("owned-node health probe retry required: {error}");
                    (node_index, false)
                }
            };
        if found_healthy {
            delay = Duration::from_secs(1);
            print_selected_node(&options.gate.urls[next_index]).await;
            tokio::time::sleep(Duration::from_millis(250)).await;
        } else {
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(Duration::from_secs(30));
        }
        node_index = next_index;
        if let Err(error) =
            resnapshot(&rest, &options.addresses, &mut projection, &mut ledger).await
        {
            eprintln!("REST resnapshot retry required: {error}");
            continue;
        }
        let replay = match replay_pending_after_resnapshot(
            &mut journal,
            &mut ledger,
            &mut projection,
            &source,
            &options.addresses,
        ) {
            Ok(replay) => replay,
            Err(error) => {
                eprintln!("durable replay retry required: {error}");
                continue;
            }
        };
        println!(
            "resnapshot  live={} checkpoint={}",
            replay.live_utxos, replay.checkpoint
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str = "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt";
    const ADDRESS_2: &str =
        "kaspatest:qqmstl2znv9tsfgcmj9shme82my867tapz7pdu4ztwdn6sm9452jj5mm0sxzw";

    fn watched() -> Vec<String> {
        vec![ADDRESS.into()]
    }

    #[test]
    fn cli_restricts_cleartext_wrpc_to_loopback() {
        assert!(kaspa_frontier_engine::require_loopback_wrpc_url("ws://127.0.0.1:18210").is_ok());
        assert!(kaspa_frontier_engine::require_loopback_wrpc_url("ws://[::1]:18210").is_ok());
        assert!(kaspa_frontier_engine::require_loopback_wrpc_url("ws://192.0.2.1:18210").is_err());
        assert!(kaspa_frontier_engine::require_loopback_wrpc_url("wss://example.com").is_err());
        assert!(
            kaspa_frontier_engine::require_loopback_wrpc_url("ws://user@127.0.0.1:18210").is_err()
        );
    }

    #[test]
    fn cli_accepts_safe_resnapshot_mode() {
        let options = parse_args([
            ADDRESS.into(),
            "--resnapshot-only".into(),
            "--database".into(),
            "state.sqlite".into(),
        ])
        .unwrap();
        assert!(options.resnapshot_only);
        assert_eq!(options.gate.urls, vec!["ws://127.0.0.1:18210"]);
        let dual = parse_args([ADDRESS.into(), "--dual".into(), "--resnapshot-only".into()]).unwrap();
        assert_eq!(dual.gate.urls.len(), 2);
        assert_eq!(options.gate.max_daa_lag, 100);
        assert_eq!(options.addresses, watched());
    }

    #[test]
    fn cli_accepts_bounded_unique_address_batches() {
        let options = parse_args([
            ADDRESS.into(),
            ADDRESS_2.into(),
            "--url".into(),
            "ws://127.0.0.1:18211".into(),
            "--url".into(),
            "ws://127.0.0.1:18210".into(),
        ])
        .unwrap();
        assert_eq!(options.addresses, vec![ADDRESS, ADDRESS_2]);
        assert_eq!(
            options.gate.urls,
            vec!["ws://127.0.0.1:18211", "ws://127.0.0.1:18210"]
        );
        assert!(parse_args([ADDRESS.into(), ADDRESS.into()]).is_err());
        assert!(parse_args([
            ADDRESS.into(),
            "--url".into(),
            "ws://127.0.0.1:18210".into(),
            "--url".into(),
            "ws://127.0.0.1:18210".into(),
        ])
        .is_err());
        assert_eq!(
            source_for_addresses(&[ADDRESS.into(), ADDRESS_2.into()]),
            source_for_addresses(&[ADDRESS_2.into(), ADDRESS.into()])
        );
        assert_ne!(
            source_for_addresses(&[ADDRESS.into()]),
            source_for_addresses(&[ADDRESS.into(), ADDRESS_2.into()])
        );
        assert!(parse_args([ADDRESS.into(), "--max-daa-lag".into(), "0".into()]).is_err());
    }

    fn assessment(behind: u64) -> OwnedNodeAssessment {
        OwnedNodeAssessment {
            local_daa: 100 - behind,
            public_daa: 100,
            behind_public_daa: behind,
            server_dag_delta: 0,
            header_body_gap: 0,
            connected_peers: 3,
            ibd_peers: 0,
        }
    }

    #[test]
    fn live_ingest_skips_unhealthy_and_holds_when_none_remain() {
        assert_eq!(hold_or_select_healthy(0, &[]), (0, false));
        let healthy = vec![(0, assessment(9)), (1, assessment(1))];
        assert_eq!(hold_or_select_healthy(0, &healthy), (1, true));
    }

    #[test]
    fn replay_after_resnapshot_applies_buffered_frames_to_ledger() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("live.sqlite");
        let mut journal = WrpcJournal::open(&database).unwrap();
        let raw = include_str!("../../fixtures/wrpc-utxos-replay.jsonl");
        for (index, line) in raw.lines().enumerate() {
            journal
                .record("live", u64::try_from(index + 1).unwrap(), line)
                .unwrap();
        }
        let mut ledger = DepositLedger::open(&database).unwrap();
        let mut projection = WrpcDepositProjection::default();
        projection.bootstrap(160, &[], ADDRESS).unwrap();
        let report = replay_pending_after_resnapshot(
            &mut journal,
            &mut ledger,
            &mut projection,
            "live",
            &watched(),
        )
        .unwrap();
        assert_eq!(report.checkpoint, 4);
        assert!(
            !ledger.unacknowledged_events().unwrap().is_empty(),
            "buffered frames must reconcile into the ledger after REST resnapshot"
        );
    }

    #[test]
    fn subscribe_ack_buffer_replays_after_crash_before_scan_apply() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("ack-crash.sqlite");
        let addresses = watched();
        let source = source_for_addresses(&addresses);
        let added = include_str!("../../fixtures/wrpc-utxos-replay.jsonl")
            .lines()
            .nth(1)
            .unwrap();
        {
            let mut journal = WrpcJournal::open(&database).unwrap();
            journal.append(&source, added).unwrap();
        }
        let mut journal = WrpcJournal::open(&database).unwrap();
        let mut ledger = DepositLedger::open(&database).unwrap();
        let mut projection = WrpcDepositProjection::default();
        projection.bootstrap(160, &[], ADDRESS).unwrap();
        let report = replay_pending_after_resnapshot(
            &mut journal,
            &mut ledger,
            &mut projection,
            &source,
            &addresses,
        )
        .unwrap();
        assert_eq!(report.checkpoint, 1);
        assert!(
            !ledger.unacknowledged_events().unwrap().is_empty(),
            "subscribe-ack journal append must survive crash before scan apply"
        );
    }

    #[test]
    fn live_no_op_daa_frame_is_atomically_checkpointed() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("delta.sqlite");
        let mut journal = WrpcJournal::open(&database).unwrap();
        let mut ledger = DepositLedger::open(&database).unwrap();
        let mut projection = WrpcDepositProjection::default();
        let mut fixture = include_str!("../../fixtures/wrpc-utxos-replay.jsonl").lines();
        let daa = fixture.next().unwrap();
        let added = fixture.next().unwrap();

        apply_live_frame(
            daa,
            &mut journal,
            &mut ledger,
            &mut projection,
            "live",
            &watched(),
        )
        .unwrap();
        assert_eq!(journal.checkpoint("live").unwrap(), 1);
        assert_eq!(ledger.pending_count().unwrap(), 0);

        apply_live_frame(
            added,
            &mut journal,
            &mut ledger,
            &mut projection,
            "live",
            &watched(),
        )
        .unwrap();
        assert_eq!(journal.checkpoint("live").unwrap(), 2);
        assert_eq!(ledger.pending_count().unwrap(), 2);

        let no_op = r#"{"method":"virtualDaaScoreChangedNotification","params":{"VirtualDaaScoreChanged":{"virtualDaaScore":"101"}}}"#;
        apply_live_frame(
            no_op,
            &mut journal,
            &mut ledger,
            &mut projection,
            "live",
            &watched(),
        )
        .unwrap();
        assert_eq!(journal.checkpoint("live").unwrap(), 3);
        assert_eq!(ledger.pending_count().unwrap(), 2);
        assert!(ledger.unacknowledged_events().unwrap().is_empty());
    }

    #[test]
    fn multi_address_restart_replays_without_cross_credit() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("multi.sqlite");
        let addresses = vec![ADDRESS.into(), ADDRESS_2.into()];
        let source = source_for_addresses(&addresses);
        {
            let mut journal = WrpcJournal::open(&database).unwrap();
            let mut ledger = DepositLedger::open(&database).unwrap();
            let mut projection = WrpcDepositProjection::default();
            let mut fixture = include_str!("../../fixtures/wrpc-utxos-replay.jsonl").lines();
            let daa = fixture.next().unwrap();
            let added = fixture.next().unwrap().replacen(ADDRESS, ADDRESS_2, 1);
            apply_live_frame(
                daa,
                &mut journal,
                &mut ledger,
                &mut projection,
                &source,
                &addresses,
            )
            .unwrap();
            apply_live_frame(
                &added,
                &mut journal,
                &mut ledger,
                &mut projection,
                &source,
                &addresses,
            )
            .unwrap();
            assert_eq!(ledger.pending_count().unwrap(), 2);
            assert_eq!(
                ledger
                    .pending_outpoints_for_addresses(&[ADDRESS.into()])
                    .unwrap(),
                vec![("b".repeat(64), 1)]
            );
            assert_eq!(
                ledger
                    .pending_outpoints_for_addresses(&[ADDRESS_2.into()])
                    .unwrap(),
                vec![("a".repeat(64), 0)]
            );
        }

        let mut journal = WrpcJournal::open(&database).unwrap();
        let mut ledger = DepositLedger::open(&database).unwrap();
        let report = kaspa_frontier_engine::replay_into_ledger_addresses(
            &mut journal,
            &mut ledger,
            &source,
            &addresses,
        )
        .unwrap();
        assert_eq!(report.checkpoint, 2);
        assert_eq!(report.live_utxos, 2);
        assert_eq!(ledger.pending_count().unwrap(), 2);
        assert!(ledger.unacknowledged_events().unwrap().is_empty());
    }

    #[test]
    #[ignore = "disk-backed throughput guard; run explicitly for performance validation"]
    fn durable_no_op_frames_sustain_tn10_rate() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("throughput.sqlite");
        let mut journal = WrpcJournal::open(&database).unwrap();
        let mut ledger = DepositLedger::open(&database).unwrap();
        let mut projection = WrpcDepositProjection::default();
        let initial = r#"{"method":"virtualDaaScoreChangedNotification","params":{"VirtualDaaScoreChanged":{"virtualDaaScore":"100"}}}"#;
        apply_live_frame(
            initial,
            &mut journal,
            &mut ledger,
            &mut projection,
            "throughput",
            &watched(),
        )
        .unwrap();

        let started = std::time::Instant::now();
        for virtual_daa_score in 101..=200 {
            let raw = format!(
                r#"{{"method":"virtualDaaScoreChangedNotification","params":{{"VirtualDaaScoreChanged":{{"virtualDaaScore":"{virtual_daa_score}"}}}}}}"#
            );
            apply_live_frame(
                &raw,
                &mut journal,
                &mut ledger,
                &mut projection,
                "throughput",
                &watched(),
            )
            .unwrap();
        }
        let elapsed = started.elapsed();
        let frames_per_second = 100.0 / elapsed.as_secs_f64();
        eprintln!("durable no-op ingestion: {frames_per_second:.1} frames/s");
        assert!(frames_per_second >= 10.0);
        assert_eq!(journal.checkpoint("throughput").unwrap(), 101);
        assert_eq!(ledger.pending_count().unwrap(), 0);
    }
}
