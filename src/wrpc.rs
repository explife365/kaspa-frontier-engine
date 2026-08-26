//! Strict, fixture-testable decoding and durable replay for kaspad wRPC JSON
//! notifications. This module deliberately does not open a WebSocket: production
//! custody must connect it to an owned `kaspad --utxoindex` and resnapshot on every
//! reconnect before accepting live notifications.

use crate::deposit_ledger::DepositLedger;
use crate::error::{EngineError, Result};
use crate::exchange::{ConfirmedDeposit, UtxoAppearance};
use crate::network::{
    is_valid_testnet_address, COINBASE_MATURITY_DAA, DEFAULT_DEPOSIT_CONFIRMATIONS,
};
use crate::rest::AddressUtxo;
use crate::rpc::{RpcOutpoint, RpcScriptPublicKey, RpcUtxoEntry, RpcUtxoEntryRef};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::Duration;

pub const MAX_WRPC_FRAME_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_WRPC_ENTRIES: usize = 10_000;
const WRPC_SCHEMA_VERSION: i64 = 1;

#[derive(Serialize)]
struct WrpcClientMessage<T> {
    id: u64,
    method: &'static str,
    params: T,
}

pub fn encode_notify_utxos_changed(id: u64, addresses: &[String]) -> Result<String> {
    if addresses.is_empty() || addresses.len() > 100 {
        return Err(EngineError::Message(
            "wRPC UTXO subscription requires 1-100 addresses".into(),
        ));
    }
    let mut unique = HashSet::with_capacity(addresses.len());
    for address in addresses {
        if !is_valid_testnet_address(address) {
            return Err(EngineError::NotTestnetAddress(address.clone()));
        }
        if !unique.insert(address) {
            return Err(EngineError::Message(
                "wRPC UTXO subscription contains a duplicate address".into(),
            ));
        }
    }
    Ok(serde_json::to_string(&WrpcClientMessage {
        id,
        method: "subscribe",
        params: serde_json::json!({"UtxosChanged": {"addresses": addresses}}),
    })?)
}

pub fn encode_notify_virtual_daa_score_changed(id: u64) -> Result<String> {
    Ok(serde_json::to_string(&WrpcClientMessage {
        id,
        method: "subscribe",
        params: serde_json::json!({"VirtualDaaScoreChanged": {}}),
    })?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrpcNotification {
    UtxosChanged {
        added: Vec<RpcUtxoEntryRef>,
        removed: Vec<RpcUtxoEntryRef>,
    },
    VirtualDaaScoreChanged {
        virtual_daa_score: u64,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WrpcServerMessage {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
    #[serde(default)]
    error: Option<WrpcServerError>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WrpcServerError {
    code: i32,
    message: String,
    #[serde(default)]
    data: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UtxosChangedParams {
    added: Vec<RpcUtxoEntryRef>,
    removed: Vec<RpcUtxoEntryRef>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VirtualDaaScoreChangedParams {
    #[serde(deserialize_with = "crate::rest::de_u64_from_string_or_number")]
    virtual_daa_score: u64,
}

pub fn decode_notification(raw: &str) -> Result<WrpcNotification> {
    if raw.len() > MAX_WRPC_FRAME_BYTES {
        return Err(EngineError::Message(format!(
            "wRPC frame exceeds {MAX_WRPC_FRAME_BYTES} bytes"
        )));
    }
    let message: WrpcServerMessage = serde_json::from_str(raw)?;
    if message.id.is_some() {
        return Err(EngineError::Message(
            "wRPC notification must not contain a request id".into(),
        ));
    }
    if let Some(error) = message.error {
        let data = error
            .data
            .map(|value| format!(" data={value}"))
            .unwrap_or_default();
        return Err(EngineError::Message(format!(
            "wRPC server error [{}]: {}{data}",
            error.code, error.message
        )));
    }
    let method = message
        .method
        .ok_or_else(|| EngineError::Message("wRPC notification is missing method".into()))?;
    let payload = message
        .params
        .ok_or_else(|| EngineError::Message("wRPC notification is missing params".into()))?;
    match method.as_str() {
        "utxosChangedNotification" => {
            let params: UtxosChangedParams =
                serde_json::from_value(exact_notification_variant(payload, "UtxosChanged")?)?;
            validate_utxo_change(&params.added, &params.removed)?;
            Ok(WrpcNotification::UtxosChanged {
                added: params.added,
                removed: params.removed,
            })
        }
        "virtualDaaScoreChangedNotification" => {
            let params: VirtualDaaScoreChangedParams = serde_json::from_value(
                exact_notification_variant(payload, "VirtualDaaScoreChanged")?,
            )?;
            Ok(WrpcNotification::VirtualDaaScoreChanged {
                virtual_daa_score: params.virtual_daa_score,
            })
        }
        _ => Err(EngineError::Message(format!(
            "unsupported wRPC notification method {method}"
        ))),
    }
}

fn exact_notification_variant(payload: Value, expected: &str) -> Result<Value> {
    let mut object = payload
        .as_object()
        .filter(|object| object.len() == 1)
        .cloned()
        .ok_or_else(|| {
            EngineError::Message("wRPC notification params must contain one variant".into())
        })?;
    object.remove(expected).ok_or_else(|| {
        EngineError::Message(format!(
            "wRPC notification params are missing {expected} variant"
        ))
    })
}

pub fn validate_subscription_ack(raw: &str, expected_id: u64, expected_method: &str) -> Result<()> {
    if raw.len() > MAX_WRPC_FRAME_BYTES {
        return Err(EngineError::Message(
            "wRPC response frame is oversized".into(),
        ));
    }
    let message: WrpcServerMessage = serde_json::from_str(raw)?;
    if let Some(error) = message.error {
        return Err(EngineError::Message(format!(
            "wRPC subscription error [{}]: {}",
            error.code, error.message
        )));
    }
    if message.id != Some(expected_id) {
        return Err(EngineError::Message(format!(
            "unexpected wRPC subscription response id {:?}",
            message.id
        )));
    }
    if message.method.as_deref() != Some(expected_method) {
        return Err(EngineError::Message(format!(
            "unexpected wRPC subscription response method {:?}",
            message.method
        )));
    }
    let _subscription_id = message
        .params
        .as_ref()
        .and_then(Value::as_object)
        .filter(|params| params.len() == 1)
        .and_then(|params| params.get("id"))
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            EngineError::Message(
                "wRPC subscription response params must contain only numeric id".into(),
            )
        })?;
    Ok(())
}

fn validate_utxo_change(added: &[RpcUtxoEntryRef], removed: &[RpcUtxoEntryRef]) -> Result<()> {
    if added.len().saturating_add(removed.len()) > MAX_WRPC_ENTRIES {
        return Err(EngineError::Message(format!(
            "wRPC UTXO notification exceeds {MAX_WRPC_ENTRIES} entries"
        )));
    }
    let mut outpoints = HashSet::with_capacity(added.len().saturating_add(removed.len()));
    for entry in added.iter().chain(removed) {
        validate_entry(entry)?;
        let key = (&entry.outpoint.transaction_id, entry.outpoint.index);
        if !outpoints.insert(key) {
            return Err(EngineError::Message(
                "wRPC notification contains a duplicate or overlapping outpoint".into(),
            ));
        }
    }
    Ok(())
}

fn validate_entry(entry: &RpcUtxoEntryRef) -> Result<()> {
    let address = entry
        .address
        .as_deref()
        .ok_or_else(|| EngineError::Message("wRPC UTXO entry is missing address".into()))?;
    if !is_valid_testnet_address(address) {
        return Err(EngineError::NotTestnetAddress(address.into()));
    }
    if !is_hash(&entry.outpoint.transaction_id) {
        return Err(EngineError::Message(
            "wRPC UTXO transactionId must be exactly 32 bytes of hex".into(),
        ));
    }
    if entry.utxo_entry.amount == 0 {
        return Err(EngineError::Message(
            "wRPC UTXO amount must be positive".into(),
        ));
    }
    let script = entry
        .utxo_entry
        .script_public_key
        .script_public_key
        .as_deref()
        .ok_or_else(|| EngineError::Message("wRPC UTXO is missing scriptPublicKey".into()))?;
    if script.is_empty()
        || script.len() % 2 != 0
        || !script.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(EngineError::Message(
            "wRPC scriptPublicKey must be non-empty even-length hex".into(),
        ));
    }
    if let Some(covenant_id) = &entry.utxo_entry.covenant_id {
        if !is_hash(covenant_id) {
            return Err(EngineError::Message(
                "wRPC covenantId must be exactly 32 bytes of hex".into(),
            ));
        }
    }
    Ok(())
}

fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrpcFrame {
    pub source: String,
    pub sequence: u64,
    pub raw_json: String,
}

pub struct WrpcJournal {
    connection: Connection,
}

impl WrpcJournal {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        let has_meta: bool = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master
                WHERE type='table' AND name='wrpc_schema_meta'
            )",
            [],
            |row| row.get(0),
        )?;
        if has_meta {
            let version: i64 = connection.query_row(
                "SELECT value FROM wrpc_schema_meta WHERE key='version'",
                [],
                |row| row.get(0),
            )?;
            if version != WRPC_SCHEMA_VERSION {
                return Err(EngineError::Message(format!(
                    "unsupported wRPC journal schema {version}; expected {WRPC_SCHEMA_VERSION}"
                )));
            }
        } else {
            for table in ["wrpc_frames", "wrpc_checkpoint"] {
                let exists: bool = connection.query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1
                    )",
                    [table],
                    |row| row.get(0),
                )?;
                if exists {
                    return Err(EngineError::Message(
                        "unversioned wRPC journal tables exist".into(),
                    ));
                }
            }
        }
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS wrpc_schema_meta(
                key TEXT PRIMARY KEY,
                value INTEGER NOT NULL
             );
             INSERT OR IGNORE INTO wrpc_schema_meta(key, value) VALUES('version', 1);
             CREATE TABLE IF NOT EXISTS wrpc_frames(
                source TEXT NOT NULL,
                sequence INTEGER NOT NULL CHECK(sequence > 0),
                raw_json TEXT NOT NULL,
                PRIMARY KEY(source, sequence)
             );
             CREATE TABLE IF NOT EXISTS wrpc_checkpoint(
                source TEXT PRIMARY KEY,
                sequence INTEGER NOT NULL CHECK(sequence >= 0)
             );",
        )?;
        require_columns(
            &connection,
            "wrpc_frames",
            &["source", "sequence", "raw_json"],
        )?;
        require_columns(&connection, "wrpc_checkpoint", &["source", "sequence"])?;
        Ok(Self { connection })
    }

    /// Record a validated frame at an explicit, gap-free source sequence.
    /// Returns false only when the exact frame was already stored.
    pub fn record(&mut self, source: &str, sequence: u64, raw_json: &str) -> Result<bool> {
        validate_source(source)?;
        if sequence == 0 {
            return Err(EngineError::Message(
                "wRPC journal sequence must be positive".into(),
            ));
        }
        decode_notification(raw_json)?;
        let sequence = to_i64(sequence, "wRPC sequence")?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<String> = transaction
            .query_row(
                "SELECT raw_json FROM wrpc_frames WHERE source=?1 AND sequence=?2",
                params![source, sequence],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            if existing == raw_json {
                transaction.commit()?;
                return Ok(false);
            }
            return Err(EngineError::Message(format!(
                "wRPC source {source} sequence {sequence} conflicts with durable journal"
            )));
        }
        let expected: i64 = transaction.query_row(
            "SELECT MAX(
                COALESCE((SELECT MAX(sequence) FROM wrpc_frames WHERE source=?1), 0),
                COALESCE((SELECT sequence FROM wrpc_checkpoint WHERE source=?1), 0)
             ) + 1",
            [source],
            |row| row.get(0),
        )?;
        if sequence != expected {
            return Err(EngineError::Message(format!(
                "wRPC source {source} sequence gap: got {sequence}, expected {expected}"
            )));
        }
        transaction.execute(
            "INSERT INTO wrpc_frames(source, sequence, raw_json) VALUES(?1, ?2, ?3)",
            params![source, sequence, raw_json],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    /// Append one validated WebSocket frame and return its durable local sequence.
    pub fn append(&mut self, source: &str, raw_json: &str) -> Result<u64> {
        validate_source(source)?;
        decode_notification(raw_json)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let sequence: i64 = transaction.query_row(
            "SELECT MAX(
                COALESCE((SELECT MAX(sequence) FROM wrpc_frames WHERE source=?1), 0),
                COALESCE((SELECT sequence FROM wrpc_checkpoint WHERE source=?1), 0)
             ) + 1",
            [source],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO wrpc_frames(source, sequence, raw_json) VALUES(?1, ?2, ?3)",
            params![source, sequence, raw_json],
        )?;
        transaction.commit()?;
        u64::try_from(sequence)
            .map_err(|_| EngineError::Message("negative wRPC journal sequence".into()))
    }

    pub fn prune_applied(&mut self, source: &str, retain: u64) -> Result<usize> {
        validate_source(source)?;
        let checkpoint = self.checkpoint(source)?;
        let cutoff = checkpoint.saturating_sub(retain);
        let cutoff = to_i64(cutoff, "wRPC prune cutoff")?;
        let deleted = self.connection.execute(
            "DELETE FROM wrpc_frames WHERE source=?1 AND sequence<=?2",
            params![source, cutoff],
        )?;
        Ok(deleted)
    }

    pub fn frames(&self, source: &str) -> Result<Vec<WrpcFrame>> {
        validate_source(source)?;
        let mut statement = self.connection.prepare(
            "SELECT sequence, raw_json FROM wrpc_frames
             WHERE source=?1 ORDER BY sequence ASC",
        )?;
        let rows = statement.query_map([source], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut frames = Vec::new();
        for row in rows {
            let (sequence, raw_json) = row?;
            frames.push(WrpcFrame {
                source: source.into(),
                sequence: u64::try_from(sequence)
                    .map_err(|_| EngineError::Message("negative wRPC journal sequence".into()))?,
                raw_json,
            });
        }
        Ok(frames)
    }

    pub fn checkpoint(&self, source: &str) -> Result<u64> {
        validate_source(source)?;
        let sequence: Option<i64> = self
            .connection
            .query_row(
                "SELECT sequence FROM wrpc_checkpoint WHERE source=?1",
                [source],
                |row| row.get(0),
            )
            .optional()?;
        u64::try_from(sequence.unwrap_or(0))
            .map_err(|_| EngineError::Message("negative wRPC checkpoint".into()))
    }

    pub fn mark_applied(&mut self, source: &str, sequence: u64) -> Result<()> {
        validate_source(source)?;
        if sequence == 0 {
            return Err(EngineError::Message(
                "wRPC checkpoint sequence must be positive".into(),
            ));
        }
        let sequence = to_i64(sequence, "wRPC sequence")?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let checkpoint: i64 = transaction
            .query_row(
                "SELECT sequence FROM wrpc_checkpoint WHERE source=?1",
                [source],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0);
        if sequence <= checkpoint {
            transaction.commit()?;
            return Ok(());
        }
        if sequence != checkpoint + 1 {
            return Err(EngineError::Message(format!(
                "wRPC checkpoint gap: got {sequence}, expected {}",
                checkpoint + 1
            )));
        }
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM wrpc_frames WHERE source=?1 AND sequence=?2
            )",
            params![source, sequence],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(EngineError::Message(
                "cannot checkpoint a missing wRPC frame".into(),
            ));
        }
        transaction.execute(
            "INSERT INTO wrpc_checkpoint(source, sequence) VALUES(?1, ?2)
             ON CONFLICT(source) DO UPDATE SET sequence=excluded.sequence",
            params![source, sequence],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

fn require_columns(connection: &Connection, table: &str, required: &[&str]) -> Result<()> {
    let sql = format!("PRAGMA table_info({table})");
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    let columns = rows.collect::<std::result::Result<HashSet<_>, _>>()?;
    for name in required {
        if !columns.contains(*name) {
            return Err(EngineError::Message(format!(
                "wRPC journal table {table} is missing required column {name}"
            )));
        }
    }
    Ok(())
}

fn validate_source(source: &str) -> Result<()> {
    if source.trim().is_empty() || source.len() > 256 {
        return Err(EngineError::Message(
            "wRPC journal source must be 1-256 bytes".into(),
        ));
    }
    Ok(())
}

fn to_i64(value: u64, label: &str) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| EngineError::Message(format!("{label} exceeds SQLite INTEGER range")))
}

#[derive(Debug, Clone)]
pub struct WrpcDepositSnapshot {
    pub virtual_daa: u64,
    pub observed: Vec<UtxoAppearance>,
    pub confirmed: Vec<ConfirmedDeposit>,
    pub disappeared_outpoints: Vec<(String, u32)>,
}

#[derive(Debug, Default)]
pub struct WrpcDepositProjection {
    virtual_daa: Option<u64>,
    live: BTreeMap<(String, u32), RpcUtxoEntryRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrpcReplayReport {
    pub checkpoint: u64,
    pub frame_count: usize,
    pub live_utxos: usize,
}

/// Rebuild projection state from the full append-only journal, then apply only
/// frames beyond the durable checkpoint to the idempotent deposit ledger.
pub fn replay_into_ledger(
    journal: &mut WrpcJournal,
    ledger: &mut DepositLedger,
    source: &str,
    watched_address: &str,
) -> Result<WrpcReplayReport> {
    let starting_checkpoint = journal.checkpoint(source)?;
    let frames = journal.frames(source)?;
    let mut projection = WrpcDepositProjection::default();
    for frame in &frames {
        let notification = decode_notification(&frame.raw_json)?;
        let snapshot = projection.apply(notification, watched_address)?;
        if frame.sequence <= starting_checkpoint {
            continue;
        }
        if let Some(snapshot) = snapshot {
            ledger.reconcile(
                snapshot.virtual_daa,
                &snapshot.observed,
                &snapshot.confirmed,
                &snapshot.disappeared_outpoints,
            )?;
        }
        journal.mark_applied(source, frame.sequence)?;
    }
    Ok(WrpcReplayReport {
        checkpoint: journal.checkpoint(source)?,
        frame_count: frames.len(),
        live_utxos: projection.live_count(),
    })
}

impl WrpcDepositProjection {
    pub fn bootstrap(
        &mut self,
        virtual_daa: u64,
        utxos: &[AddressUtxo],
        watched_address: &str,
    ) -> Result<WrpcDepositSnapshot> {
        if !is_valid_testnet_address(watched_address) {
            return Err(EngineError::NotTestnetAddress(watched_address.into()));
        }
        let previous: HashSet<_> = self.live.keys().cloned().collect();
        let mut replacement = BTreeMap::new();
        for utxo in utxos {
            if utxo.address != watched_address {
                return Err(EngineError::Message(format!(
                    "REST resnapshot returned foreign address {}",
                    utxo.address
                )));
            }
            let entry = RpcUtxoEntryRef {
                address: Some(utxo.address.clone()),
                outpoint: RpcOutpoint {
                    transaction_id: utxo.outpoint.transaction_id.clone(),
                    index: utxo.outpoint.index,
                },
                utxo_entry: RpcUtxoEntry {
                    amount: utxo.utxo_entry.amount,
                    script_public_key: RpcScriptPublicKey {
                        script_public_key: utxo
                            .utxo_entry
                            .script_public_key
                            .script_public_key
                            .clone(),
                        version: utxo.utxo_entry.script_public_key.version,
                    },
                    block_daa_score: utxo.utxo_entry.block_daa_score,
                    is_coinbase: utxo.utxo_entry.is_coinbase,
                    covenant_id: utxo.utxo_entry.covenant_id.clone(),
                    storage_mass: utxo.utxo_entry.storage_mass,
                },
            };
            validate_entry(&entry)?;
            let key = (entry.outpoint.transaction_id.clone(), entry.outpoint.index);
            if replacement.insert(key, entry).is_some() {
                return Err(EngineError::Message(
                    "REST resnapshot contains duplicate outpoint".into(),
                ));
            }
        }
        let current: HashSet<_> = replacement.keys().cloned().collect();
        let disappeared = previous.difference(&current).cloned().collect();
        self.virtual_daa = Some(virtual_daa);
        self.live = replacement;
        self.snapshot(disappeared)?
            .ok_or_else(|| EngineError::Message("REST resnapshot is missing virtual DAA".into()))
    }

    pub fn apply(
        &mut self,
        notification: WrpcNotification,
        watched_address: &str,
    ) -> Result<Option<WrpcDepositSnapshot>> {
        self.apply_strict(notification, watched_address)
    }

    /// Validate a pending frame after a fresh REST resnapshot without mutating the
    /// projection. The complete snapshot subsumes every pre-reconnect delta.
    pub fn apply_after_resnapshot(
        &mut self,
        notification: WrpcNotification,
        watched_address: &str,
    ) -> Result<Option<WrpcDepositSnapshot>> {
        if !is_valid_testnet_address(watched_address) {
            return Err(EngineError::NotTestnetAddress(watched_address.into()));
        }
        if let WrpcNotification::UtxosChanged { added, removed } = notification {
            for entry in added.iter().chain(&removed) {
                require_watched(entry, watched_address)?;
            }
        }
        self.snapshot(Vec::new())
    }

    fn apply_strict(
        &mut self,
        notification: WrpcNotification,
        watched_address: &str,
    ) -> Result<Option<WrpcDepositSnapshot>> {
        if !is_valid_testnet_address(watched_address) {
            return Err(EngineError::NotTestnetAddress(watched_address.into()));
        }
        let mut disappeared = Vec::new();
        match notification {
            WrpcNotification::VirtualDaaScoreChanged { virtual_daa_score } => {
                if self
                    .virtual_daa
                    .is_some_and(|previous| virtual_daa_score < previous)
                {
                    return Err(EngineError::Message(
                        "wRPC virtual DAA score moved backwards; resnapshot required".into(),
                    ));
                }
                self.virtual_daa = Some(virtual_daa_score);
            }
            WrpcNotification::UtxosChanged { added, removed } => {
                for entry in removed {
                    require_watched(&entry, watched_address)?;
                    let key = (entry.outpoint.transaction_id.clone(), entry.outpoint.index);
                    let Some(existing) = self.live.get(&key) else {
                        disappeared.push(key);
                        continue;
                    };
                    if !same_entry(existing, &entry) {
                        return Err(EngineError::Message(format!(
                            "wRPC removal facts changed for {}:{}",
                            key.0, key.1
                        )));
                    }
                    self.live.remove(&key);
                    disappeared.push(key);
                }
                for entry in added {
                    require_watched(&entry, watched_address)?;
                    let key = (entry.outpoint.transaction_id.clone(), entry.outpoint.index);
                    if let Some(existing) = self.live.get(&key) {
                        if !same_entry(existing, &entry) {
                            return Err(EngineError::Message(format!(
                                "wRPC addition facts changed for {}:{}",
                                key.0, key.1
                            )));
                        }
                    } else {
                        self.live.insert(key, entry);
                    }
                }
            }
        }
        self.snapshot(disappeared)
    }

    fn snapshot(&self, disappeared: Vec<(String, u32)>) -> Result<Option<WrpcDepositSnapshot>> {
        let Some(virtual_daa) = self.virtual_daa else {
            return Ok(None);
        };
        let mut observed = Vec::with_capacity(self.live.len());
        let mut confirmed = Vec::new();
        for entry in self.live.values() {
            let appearance = to_appearance(entry, virtual_daa)?;
            let confirmations = virtual_daa.saturating_sub(appearance.block_daa_score);
            let required = if appearance.is_coinbase {
                DEFAULT_DEPOSIT_CONFIRMATIONS.max(COINBASE_MATURITY_DAA)
            } else {
                DEFAULT_DEPOSIT_CONFIRMATIONS.max(1)
            };
            if confirmations >= required {
                confirmed.push(ConfirmedDeposit {
                    tx_id: appearance.tx_id.clone(),
                    output_index: appearance.output_index,
                    address: appearance.address.clone(),
                    amount_sompi: appearance.amount_sompi,
                    block_daa_score: appearance.block_daa_score,
                    confirmations,
                    is_coinbase: appearance.is_coinbase,
                });
            }
            observed.push(appearance);
        }
        Ok(Some(WrpcDepositSnapshot {
            virtual_daa,
            observed,
            confirmed,
            disappeared_outpoints: disappeared,
        }))
    }

    pub fn live_count(&self) -> usize {
        self.live.len()
    }
}

fn require_watched(entry: &RpcUtxoEntryRef, watched: &str) -> Result<()> {
    if entry.address.as_deref() != Some(watched) {
        return Err(EngineError::Message(format!(
            "wRPC entry is outside watched address {watched}"
        )));
    }
    Ok(())
}

fn same_entry(left: &RpcUtxoEntryRef, right: &RpcUtxoEntryRef) -> bool {
    left.address == right.address
        && left.outpoint.transaction_id == right.outpoint.transaction_id
        && left.outpoint.index == right.outpoint.index
        && left.utxo_entry.amount == right.utxo_entry.amount
        && left.utxo_entry.block_daa_score == right.utxo_entry.block_daa_score
        && left.utxo_entry.is_coinbase == right.utxo_entry.is_coinbase
        && left.utxo_entry.script_public_key.script_public_key
            == right.utxo_entry.script_public_key.script_public_key
        && left.utxo_entry.script_public_key.version == right.utxo_entry.script_public_key.version
        && left.utxo_entry.covenant_id == right.utxo_entry.covenant_id
        && left.utxo_entry.storage_mass == right.utxo_entry.storage_mass
}

fn to_appearance(entry: &RpcUtxoEntryRef, virtual_daa: u64) -> Result<UtxoAppearance> {
    Ok(UtxoAppearance {
        tx_id: entry.outpoint.transaction_id.clone(),
        output_index: entry.outpoint.index,
        address: entry
            .address
            .clone()
            .ok_or_else(|| EngineError::Message("wRPC UTXO is missing address".into()))?,
        amount_sompi: entry.utxo_entry.amount,
        block_daa_score: entry.utxo_entry.block_daa_score,
        virtual_daa,
        is_coinbase: entry.utxo_entry.is_coinbase,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str = "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt";

    fn added_frame() -> String {
        format!(
            r#"{{
                "method":"utxosChangedNotification",
                "params":{{
                    "UtxosChanged":{{
                        "added":[{{
                            "address":"{ADDRESS}",
                            "outpoint":{{"transactionId":"{}","index":0}},
                            "utxoEntry":{{
                                "amount":"1000",
                                "scriptPublicKey":{{"scriptPublicKey":"20ab","version":0}},
                                "blockDaaScore":"100",
                                "isCoinbase":false
                            }}
                        }}],
                        "removed":[]
                    }}
                }}
            }}"#,
            "a".repeat(64)
        )
    }

    #[test]
    fn decodes_authoritative_wrpc_envelope_and_rejects_loose_shapes() {
        let notification = decode_notification(&added_frame()).unwrap();
        assert!(matches!(
            notification,
            WrpcNotification::UtxosChanged { added, .. } if added.len() == 1
        ));
        assert!(decode_notification(
            r#"{"id":1,"method":"virtualDaaScoreChangedNotification","params":{"VirtualDaaScoreChanged":{"virtualDaaScore":"1"}}}"#
        )
        .is_err());
        assert!(decode_notification(
            r#"{"method":"utxosChangedNotification","params":{"UtxosChanged":{"added":[],"removed":[],"extra":1}}}"#
        )
        .is_err());
        assert!(decode_notification(r#"{"method":"unknownNotification","params":{}}"#).is_err());
    }

    #[test]
    fn encodes_owned_node_subscription_requests_without_jsonrpc_field() {
        let utxos = encode_notify_utxos_changed(1, &[ADDRESS.into()]).unwrap();
        assert_eq!(
            utxos,
            format!(
                r#"{{"id":1,"method":"subscribe","params":{{"UtxosChanged":{{"addresses":["{ADDRESS}"]}}}}}}"#
            )
        );
        let daa = encode_notify_virtual_daa_score_changed(2).unwrap();
        assert_eq!(
            daa,
            r#"{"id":2,"method":"subscribe","params":{"VirtualDaaScoreChanged":{}}}"#
        );
        assert!(encode_notify_utxos_changed(3, &[]).is_err());
        validate_subscription_ack(
            r#"{"id":1,"method":"subscribe","params":{"id":9}}"#,
            1,
            "subscribe",
        )
        .unwrap();
        assert!(validate_subscription_ack(
            r#"{"id":2,"method":"subscribe","params":{"id":9}}"#,
            1,
            "subscribe",
        )
        .is_err());
    }

    #[test]
    fn journal_is_gap_free_idempotent_and_checkpointed() {
        let mut journal = WrpcJournal::open(":memory:").unwrap();
        let frame = added_frame();
        assert!(journal.record("fixture", 1, &frame).unwrap());
        assert!(!journal.record("fixture", 1, &frame).unwrap());
        assert!(journal.record("fixture", 3, &frame).is_err());
        assert!(journal.record("fixture", 1, "{}").is_err());
        assert!(journal.mark_applied("fixture", 2).is_err());
        journal.mark_applied("fixture", 1).unwrap();
        journal.mark_applied("fixture", 1).unwrap();
        assert_eq!(journal.checkpoint("fixture").unwrap(), 1);
        assert_eq!(journal.frames("fixture").unwrap().len(), 1);
        assert_eq!(journal.prune_applied("fixture", 0).unwrap(), 1);
        assert_eq!(journal.append("fixture", &frame).unwrap(), 2);
    }

    #[test]
    fn projection_confirms_by_daa_and_handles_unknown_removal_idempotently() {
        let mut projection = WrpcDepositProjection::default();
        assert!(projection
            .apply(decode_notification(&added_frame()).unwrap(), ADDRESS)
            .unwrap()
            .is_none());
        let daa = decode_notification(
            r#"{"method":"virtualDaaScoreChangedNotification","params":{"VirtualDaaScoreChanged":{"virtualDaaScore":"160"}}}"#,
        )
        .unwrap();
        let snapshot = projection.apply(daa, ADDRESS).unwrap().unwrap();
        assert_eq!(snapshot.observed.len(), 1);
        assert_eq!(snapshot.confirmed.len(), 1);
        assert_eq!(projection.live_count(), 1);

        let unknown = format!(
            r#"{{
                "method":"utxosChangedNotification",
                "params":{{
                    "UtxosChanged":{{
                        "added":[],
                        "removed":[{{
                            "address":"{ADDRESS}",
                            "outpoint":{{"transactionId":"{}","index":9}},
                            "utxoEntry":{{
                                "amount":"1",
                                "scriptPublicKey":{{"scriptPublicKey":"20ab","version":0}},
                                "blockDaaScore":"100",
                                "isCoinbase":false
                            }}
                        }}]
                    }}
                }}
            }}"#,
            "c".repeat(64)
        );
        let unknown = decode_notification(&unknown).unwrap();
        let snapshot = projection.apply(unknown, ADDRESS).unwrap().unwrap();
        assert_eq!(snapshot.disappeared_outpoints.len(), 1);
        assert_eq!(projection.live_count(), 1);
    }

    #[test]
    fn checked_in_replay_is_crash_safe_and_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("replay.sqlite");
        let mut journal = WrpcJournal::open(&database).unwrap();
        let raw = include_str!("../fixtures/wrpc-utxos-replay.jsonl");
        for (index, line) in raw.lines().enumerate() {
            journal
                .record("fixture", u64::try_from(index + 1).unwrap(), line)
                .unwrap();
        }
        let mut ledger = DepositLedger::open(&database).unwrap();
        let report = replay_into_ledger(&mut journal, &mut ledger, "fixture", ADDRESS).unwrap();
        assert_eq!(report.checkpoint, 4);
        assert_eq!(report.live_utxos, 1);
        assert_eq!(ledger.pending_count().unwrap(), 0);
        assert_eq!(ledger.unacknowledged_events().unwrap().len(), 2);

        let second = replay_into_ledger(&mut journal, &mut ledger, "fixture", ADDRESS).unwrap();
        assert_eq!(second, report);
        assert_eq!(ledger.unacknowledged_events().unwrap().len(), 2);
    }
}
