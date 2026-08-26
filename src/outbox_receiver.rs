//! Transactional receiver inbox for at-least-once custody webhook delivery.

use crate::error::{EngineError, Result};
use crate::network::is_valid_testnet_address;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IncomingLedgerEvent {
    pub id: i64,
    pub event_key: String,
    pub kind: String,
    pub tx_id: String,
    pub output_index: u32,
    pub amount_sompi: u64,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeliveryEnvelope {
    pub schema_version: u8,
    pub event: IncomingLedgerEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxOutcome {
    Accepted,
    Duplicate,
    Conflict,
}

pub struct OutboxReceiverStore {
    connection: Connection,
}

impl OutboxReceiverStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        let has_meta = table_exists(&connection, "receiver_schema_meta")?;
        if has_meta {
            let version: i64 = connection.query_row(
                "SELECT value FROM receiver_schema_meta WHERE key='version'",
                [],
                |row| row.get(0),
            )?;
            if version != SCHEMA_VERSION {
                return Err(EngineError::Message(format!(
                    "unsupported receiver schema {version}; expected {SCHEMA_VERSION}"
                )));
            }
        } else if table_exists(&connection, "receiver_inbox")? {
            return Err(EngineError::Message(
                "unversioned receiver inbox exists".into(),
            ));
        }
        connection.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS receiver_schema_meta(
               key TEXT PRIMARY KEY,
               value INTEGER NOT NULL
             );
             INSERT INTO receiver_schema_meta(key, value) VALUES('version', 1)
               ON CONFLICT(key) DO NOTHING;
             CREATE TABLE IF NOT EXISTS receiver_inbox(
               event_key TEXT PRIMARY KEY,
               schema_version INTEGER NOT NULL,
               kind TEXT NOT NULL CHECK(kind IN ('credit','reverse')),
               tx_id TEXT NOT NULL,
               output_index INTEGER NOT NULL,
               amount_sompi INTEGER NOT NULL,
               address TEXT NOT NULL,
               sender_event_id INTEGER NOT NULL,
               received_at INTEGER NOT NULL
             );
             COMMIT;",
        )?;
        require_columns(
            &connection,
            "receiver_inbox",
            &[
                "event_key",
                "schema_version",
                "kind",
                "tx_id",
                "output_index",
                "amount_sompi",
                "address",
                "sender_event_id",
                "received_at",
            ],
        )?;
        Ok(Self { connection })
    }

    pub fn accept(
        &mut self,
        idempotency_key: &str,
        delivery: &DeliveryEnvelope,
        received_at: u64,
    ) -> Result<InboxOutcome> {
        validate_delivery(idempotency_key, delivery)?;
        let received_at = to_i64(received_at, "receiver timestamp")?;
        let amount = to_i64(delivery.event.amount_sompi, "receiver amount")?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = transaction
            .query_row(
                "SELECT schema_version, kind, tx_id, output_index, amount_sompi, address
                 FROM receiver_inbox WHERE event_key=?1",
                [&delivery.event.event_key],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()?;
        if let Some(existing) = existing {
            transaction.commit()?;
            let same = existing.0 == i64::from(delivery.schema_version)
                && existing.1 == delivery.event.kind
                && existing.2 == delivery.event.tx_id
                && existing.3 == i64::from(delivery.event.output_index)
                && existing.4 == amount
                && existing.5 == delivery.event.address;
            return Ok(if same {
                InboxOutcome::Duplicate
            } else {
                InboxOutcome::Conflict
            });
        }
        transaction.execute(
            "INSERT INTO receiver_inbox(
               event_key, schema_version, kind, tx_id, output_index,
               amount_sompi, address, sender_event_id, received_at
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                delivery.event.event_key,
                i64::from(delivery.schema_version),
                delivery.event.kind,
                delivery.event.tx_id,
                i64::from(delivery.event.output_index),
                amount,
                delivery.event.address,
                delivery.event.id,
                received_at,
            ],
        )?;
        transaction.commit()?;
        Ok(InboxOutcome::Accepted)
    }

    pub fn count(&self) -> Result<u64> {
        let count: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM receiver_inbox", [], |row| row.get(0))?;
        u64::try_from(count).map_err(|_| EngineError::Message("invalid receiver count".into()))
    }
}

fn validate_delivery(idempotency_key: &str, delivery: &DeliveryEnvelope) -> Result<()> {
    if delivery.schema_version != 1 {
        return Err(EngineError::Message(format!(
            "unsupported delivery schema {}",
            delivery.schema_version
        )));
    }
    let event = &delivery.event;
    if event.id <= 0 {
        return Err(EngineError::Message(
            "sender event id must be positive".into(),
        ));
    }
    if !matches!(event.kind.as_str(), "credit" | "reverse") {
        return Err(EngineError::Message(
            "event kind must be credit or reverse".into(),
        ));
    }
    if event.tx_id.len() != 64
        || !event
            .tx_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(EngineError::Message(
            "event txid must be exactly 32 bytes of lowercase hex".into(),
        ));
    }
    if event.amount_sompi == 0 {
        return Err(EngineError::Message("event amount must be positive".into()));
    }
    if !is_valid_testnet_address(&event.address) {
        return Err(EngineError::NotTestnetAddress(event.address.clone()));
    }
    let expected_key = format!("{}:{}:{}", event.kind, event.tx_id, event.output_index);
    if event.event_key != expected_key {
        return Err(EngineError::Message(
            "event key does not match event facts".into(),
        ));
    }
    if idempotency_key != event.event_key {
        return Err(EngineError::Message(
            "Idempotency-Key header does not match event key".into(),
        ));
    }
    Ok(())
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?)
}

fn require_columns(connection: &Connection, table: &str, required: &[&str]) -> Result<()> {
    let sql = format!("PRAGMA table_info({table})");
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    let columns = rows.collect::<std::result::Result<std::collections::HashSet<_>, _>>()?;
    for name in required {
        if !columns.contains(*name) {
            return Err(EngineError::Message(format!(
                "receiver table {table} is missing required column {name}"
            )));
        }
    }
    Ok(())
}

fn to_i64(value: u64, label: &str) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| EngineError::Message(format!("{label} exceeds SQLite INTEGER range")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str = "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt";

    fn delivery() -> DeliveryEnvelope {
        let tx_id = "a".repeat(64);
        DeliveryEnvelope {
            schema_version: 1,
            event: IncomingLedgerEvent {
                id: 1,
                event_key: format!("credit:{tx_id}:0"),
                kind: "credit".into(),
                tx_id,
                output_index: 0,
                amount_sompi: 10,
                address: ADDRESS.into(),
            },
        }
    }

    #[test]
    fn duplicate_is_idempotent_across_restart_and_conflicts_fail_closed() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("receiver.sqlite");
        let key = delivery().event.event_key;
        {
            let mut store = OutboxReceiverStore::open(&database).unwrap();
            assert_eq!(
                store.accept(&key, &delivery(), 100).unwrap(),
                InboxOutcome::Accepted
            );
            assert_eq!(
                store.accept(&key, &delivery(), 101).unwrap(),
                InboxOutcome::Duplicate
            );
            assert_eq!(store.count().unwrap(), 1);
        }
        let mut restarted = OutboxReceiverStore::open(&database).unwrap();
        assert_eq!(
            restarted.accept(&key, &delivery(), 102).unwrap(),
            InboxOutcome::Duplicate
        );
        let mut conflict = delivery();
        conflict.event.amount_sompi += 1;
        assert_eq!(
            restarted.accept(&key, &conflict, 103).unwrap(),
            InboxOutcome::Conflict
        );
        assert_eq!(restarted.count().unwrap(), 1);
    }

    #[test]
    fn malformed_delivery_fails_before_inbox_write() {
        let mut store = OutboxReceiverStore::open(":memory:").unwrap();
        let mut invalid = delivery();
        invalid.schema_version = 2;
        assert!(store
            .accept(&invalid.event.event_key.clone(), &invalid, 100)
            .is_err());
        let invalid = delivery();
        assert!(store.accept("wrong", &invalid, 100).is_err());
        let mut invalid = delivery();
        invalid.event.address = "kaspatest:abc".into();
        assert!(store
            .accept(&invalid.event.event_key.clone(), &invalid, 100)
            .is_err());
        assert_eq!(store.count().unwrap(), 0);
    }
}
