//! Owned-node wRPC JSON ingestion for TN10 deposits.
//! Read-only: subscribes to UTXO/DAA notifications and writes only local SQLite state.

use futures_util::{SinkExt, StreamExt};
use kaspa_frontier_engine::network::{
    is_valid_testnet_address, require_tn10, TESTNET_10_REST, TN10_WRPC_JSON,
};
use kaspa_frontier_engine::{
    decode_notification, encode_notify_utxos_changed, encode_notify_virtual_daa_score_changed,
    require_loopback_wrpc_url, validate_subscription_ack, DepositLedger, Tn10RestClient,
    WrpcDepositProjection, WrpcJournal, WrpcReplayReport,
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
    address: String,
    url: String,
    rest: String,
    database: PathBuf,
    resnapshot_only: bool,
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut address = None;
    let mut url = format!("ws://127.0.0.1:{TN10_WRPC_JSON}");
    let mut rest = TESTNET_10_REST.to_string();
    let mut database = PathBuf::from(".local/tn10-wrpc-live.sqlite");
    let mut resnapshot_only = false;
    let mut args = arguments.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--url" => url = args.next().ok_or("--url needs a value")?,
            "--rest" => rest = args.next().ok_or("--rest needs a value")?,
            "--database" => database = PathBuf::from(args.next().ok_or("--database needs a path")?),
            "--resnapshot-only" => resnapshot_only = true,
            _ if argument.starts_with('-') => return Err(format!("unknown flag {argument}")),
            _ if address.is_none() => address = Some(argument),
            _ => return Err(usage().into()),
        }
    }
    let address = address.ok_or_else(|| usage().to_string())?;
    if !is_valid_testnet_address(&address) {
        return Err("ADDRESS must be a checksummed kaspatest address".into());
    }
    require_loopback_wrpc_url(&url).map_err(|error| error.to_string())?;
    Ok(Options {
        address,
        url,
        rest,
        database,
        resnapshot_only,
    })
}

fn usage() -> &'static str {
    "usage: tn10-wrpc-live ADDRESS [--url ws://127.0.0.1:18210] [--rest HTTPS] [--database PATH] [--resnapshot-only]"
}

async fn resnapshot(
    rest: &Tn10RestClient,
    address: &str,
    projection: &mut WrpcDepositProjection,
    ledger: &mut DepositLedger,
) -> kaspa_frontier_engine::Result<()> {
    let addresses = [address.to_string()];
    let (dag, utxos) = tokio::join!(rest.block_dag_info(), rest.utxos_for_addresses(&addresses));
    let dag = dag?;
    require_tn10(&dag.network_name)?;
    let mut snapshot = projection.bootstrap(dag.virtual_daa_score, &utxos?, address)?;
    let current: HashSet<_> = snapshot
        .observed
        .iter()
        .map(|entry| (entry.tx_id.clone(), entry.output_index))
        .collect();
    for pending in ledger.pending_outpoints()? {
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

fn replay_pending_after_resnapshot(
    journal: &mut WrpcJournal,
    projection: &mut WrpcDepositProjection,
    source: &str,
    address: &str,
) -> kaspa_frontier_engine::Result<WrpcReplayReport> {
    let checkpoint = journal.checkpoint(source)?;
    let frames = journal.frames(source)?;
    let mut validated_through = None;
    for frame in &frames {
        if frame.sequence <= checkpoint {
            continue;
        }
        let notification = decode_notification(&frame.raw_json)?;
        let delta = projection.apply_after_resnapshot(notification, address)?;
        debug_assert!(delta.is_none());
        validated_through = Some(frame.sequence);
    }
    if let Some(sequence) = validated_through {
        journal.mark_applied_through(source, sequence)?;
    }
    Ok(WrpcReplayReport {
        checkpoint: journal.checkpoint(source)?,
        frame_count: frames.len(),
        live_utxos: projection.live_count(),
    })
}

fn apply_live_frame(
    raw: &str,
    journal: &mut WrpcJournal,
    ledger: &mut DepositLedger,
    projection: &mut WrpcDepositProjection,
    source: &str,
    address: &str,
) -> kaspa_frontier_engine::Result<()> {
    let notification = decode_notification(raw)?;
    if let Some(snapshot) = projection.apply(notification, address)? {
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
        tokio_tungstenite::connect_async_with_config(&options.url, Some(config), false),
    )
    .await
    .map_err(|_| "wRPC connect timeout")??;

    let utxo_request = encode_notify_utxos_changed(1, std::slice::from_ref(&options.address))?;
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
            None => apply_live_frame(&raw, journal, ledger, projection, source, &options.address)?,
        }
    }
    println!("subscribed  UTXOs + virtual DAA");

    let mut resnapshot_interval = tokio::time::interval(RESNAPSHOT_INTERVAL);
    resnapshot_interval.tick().await;
    loop {
        tokio::select! {
            _ = resnapshot_interval.tick() => {
                resnapshot(rest, &options.address, projection, ledger).await?;
                println!(
                    "resnapshot  live={} checkpoint={}",
                    projection.live_count(),
                    journal.checkpoint(source)?
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
                        &options.address,
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
    let source = format!("owned-node:{}", options.address);
    let mut journal = WrpcJournal::open(&options.database)?;
    let mut ledger = DepositLedger::open(&options.database)?;
    let mut projection = WrpcDepositProjection::default();

    resnapshot(&rest, &options.address, &mut projection, &mut ledger).await?;
    let replay =
        replay_pending_after_resnapshot(&mut journal, &mut projection, &source, &options.address)?;
    println!("database    {}", options.database.display());
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

    let mut delay = Duration::from_secs(1);
    loop {
        match run_connection(
            &options,
            &rest,
            &mut journal,
            &mut ledger,
            &mut projection,
            &source,
        )
        .await
        {
            Ok(()) => unreachable!("wRPC connection loop only returns on failure"),
            Err(error) => eprintln!("wRPC reconnect required: {error}"),
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(30));
        if let Err(error) = resnapshot(&rest, &options.address, &mut projection, &mut ledger).await
        {
            eprintln!("REST resnapshot retry required: {error}");
            continue;
        }
        let replay = match replay_pending_after_resnapshot(
            &mut journal,
            &mut projection,
            &source,
            &options.address,
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

    #[test]
    fn cli_restricts_cleartext_wrpc_to_loopback() {
        assert!(require_loopback_wrpc_url("ws://127.0.0.1:18210").is_ok());
        assert!(require_loopback_wrpc_url("ws://[::1]:18210").is_ok());
        assert!(require_loopback_wrpc_url("ws://192.0.2.1:18210").is_err());
        assert!(require_loopback_wrpc_url("wss://example.com").is_err());
        assert!(require_loopback_wrpc_url("ws://user@127.0.0.1:18210").is_err());
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
        assert_eq!(options.url, "ws://127.0.0.1:18210");
    }

    #[test]
    fn replay_after_resnapshot_treats_already_removed_as_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("live.sqlite");
        let mut journal = WrpcJournal::open(&database).unwrap();
        let raw = include_str!("../../fixtures/wrpc-utxos-replay.jsonl");
        for (index, line) in raw.lines().enumerate() {
            journal
                .record("live", u64::try_from(index + 1).unwrap(), line)
                .unwrap();
        }
        let mut projection = WrpcDepositProjection::default();
        projection.bootstrap(160, &[], ADDRESS).unwrap();
        let report =
            replay_pending_after_resnapshot(&mut journal, &mut projection, "live", ADDRESS)
                .unwrap();
        assert_eq!(report.checkpoint, 4);
        assert_eq!(report.live_utxos, 0);
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
            ADDRESS,
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
            ADDRESS,
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
            ADDRESS,
        )
        .unwrap();
        assert_eq!(journal.checkpoint("live").unwrap(), 3);
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
            ADDRESS,
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
                ADDRESS,
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
