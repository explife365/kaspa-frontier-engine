//! Durable TN10 deposit rehearsal state.
//!
//! SQLite is the source of truth. Credit/reversal notifications use a durable
//! outbox keyed by outpoint so downstream consumers can de-duplicate safely.

use crate::error::{EngineError, Result};
use crate::exchange::{ConfirmedDeposit, UtxoAppearance};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::Path;
use std::time::Duration;

const SCHEMA_VERSION: i64 = 2;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEvent {
    pub id: i64,
    pub event_key: String,
    pub kind: String,
    pub tx_id: String,
    pub output_index: u32,
    pub amount_sompi: u64,
    pub address: String,
}

pub struct DepositLedger {
    conn: Connection,
}

impl DepositLedger {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        let has_meta = table_exists(&conn, "schema_meta")?;
        if has_meta {
            let version: i64 = conn.query_row(
                "SELECT value FROM schema_meta WHERE key='version'",
                [],
                |row| row.get(0),
            )?;
            match version {
                1 => migrate_v1_to_v2(&conn)?,
                SCHEMA_VERSION => {}
                _ => {
                    return Err(EngineError::Message(format!(
                        "unsupported deposit ledger schema {version}; expected {SCHEMA_VERSION}"
                    )));
                }
            }
        } else {
            for table in ["deposits", "legacy_credits", "deposit_outbox"] {
                if table_exists(&conn, table)? {
                    return Err(EngineError::Message(
                        "unversioned deposit ledger tables exist; refusing to stamp schema version 2"
                            .into(),
                    ));
                }
            }
        }
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS schema_meta (
               key TEXT PRIMARY KEY,
               value INTEGER NOT NULL
             );
             INSERT INTO schema_meta(key, value) VALUES ('version', 2)
               ON CONFLICT(key) DO NOTHING;
             CREATE TABLE IF NOT EXISTS deposits (
               tx_id TEXT NOT NULL,
               output_index INTEGER NOT NULL,
               address TEXT NOT NULL,
               amount_sompi INTEGER NOT NULL,
               block_daa_score INTEGER NOT NULL,
               is_coinbase INTEGER NOT NULL,
               state TEXT NOT NULL CHECK(state IN ('pending','credited','reversed')),
               first_seen_daa INTEGER NOT NULL,
               last_seen_daa INTEGER NOT NULL,
               credited_daa INTEGER,
               reversed_daa INTEGER,
               reversal_reason TEXT,
               PRIMARY KEY(tx_id, output_index)
             );
             CREATE INDEX IF NOT EXISTS deposits_by_state ON deposits(state);
             CREATE TABLE IF NOT EXISTS legacy_credits (
               tx_id TEXT NOT NULL,
               output_index INTEGER NOT NULL,
               PRIMARY KEY(tx_id, output_index)
             );
             CREATE TABLE IF NOT EXISTS deposit_outbox (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               event_key TEXT NOT NULL UNIQUE,
               kind TEXT NOT NULL CHECK(kind IN ('credit','reverse')),
               tx_id TEXT NOT NULL,
               output_index INTEGER NOT NULL,
               amount_sompi INTEGER NOT NULL,
               address TEXT NOT NULL,
               acknowledged INTEGER NOT NULL DEFAULT 0,
               lease_owner TEXT,
               lease_until INTEGER,
               attempts INTEGER NOT NULL DEFAULT 0,
               last_error TEXT
             );
             COMMIT;",
        )?;
        let version: i64 = conn.query_row(
            "SELECT value FROM schema_meta WHERE key='version'",
            [],
            |row| row.get(0),
        )?;
        if version != SCHEMA_VERSION {
            return Err(EngineError::Message(format!(
                "unsupported deposit ledger schema {version}; expected {SCHEMA_VERSION}"
            )));
        }
        require_columns(
            &conn,
            "deposits",
            &[
                "tx_id",
                "output_index",
                "address",
                "amount_sompi",
                "block_daa_score",
                "is_coinbase",
                "state",
                "first_seen_daa",
                "last_seen_daa",
                "credited_daa",
                "reversed_daa",
                "reversal_reason",
            ],
        )?;
        require_columns(
            &conn,
            "deposit_outbox",
            &[
                "id",
                "event_key",
                "kind",
                "tx_id",
                "output_index",
                "amount_sompi",
                "address",
                "acknowledged",
                "lease_owner",
                "lease_until",
                "attempts",
                "last_error",
            ],
        )?;
        require_columns(&conn, "legacy_credits", &["tx_id", "output_index"])?;
        Ok(Self { conn })
    }

    pub fn import_legacy_credit(&mut self, tx_id: &str, output_index: u32) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO legacy_credits(tx_id, output_index) VALUES (?1, ?2)",
            params![tx_id, i64::from(output_index)],
        )?;
        Ok(())
    }

    pub fn reconcile(
        &mut self,
        virtual_daa: u64,
        observed: &[UtxoAppearance],
        confirmed: &[ConfirmedDeposit],
        disappeared: &[(String, u32)],
    ) -> Result<()> {
        let daa = to_i64(virtual_daa, "virtual DAA")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        for item in observed {
            let key_index = i64::from(item.output_index);
            let existing: Option<(String, i64, i64, i64)> = tx
                .query_row(
                    "SELECT address, amount_sompi, block_daa_score, is_coinbase
                     FROM deposits WHERE tx_id=?1 AND output_index=?2",
                    params![item.tx_id, key_index],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;
            let amount = to_i64(item.amount_sompi, "deposit amount")?;
            let block_daa = to_i64(item.block_daa_score, "block DAA")?;
            let coinbase = i64::from(item.is_coinbase);
            if let Some((address, old_amount, old_daa, old_coinbase)) = existing {
                if address != item.address
                    || old_amount != amount
                    || old_daa != block_daa
                    || old_coinbase != coinbase
                {
                    return Err(EngineError::Message(format!(
                        "outpoint facts changed for {}:{}",
                        item.tx_id, item.output_index
                    )));
                }
                tx.execute(
                    "UPDATE deposits SET last_seen_daa=?3
                     WHERE tx_id=?1 AND output_index=?2",
                    params![item.tx_id, key_index, daa],
                )?;
            } else {
                let legacy: bool = tx.query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM legacy_credits WHERE tx_id=?1 AND output_index=?2
                     )",
                    params![item.tx_id, key_index],
                    |row| row.get(0),
                )?;
                tx.execute(
                    "INSERT INTO deposits(
                       tx_id, output_index, address, amount_sompi, block_daa_score,
                       is_coinbase, state, first_seen_daa, last_seen_daa, credited_daa
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8,?9)",
                    params![
                        item.tx_id,
                        key_index,
                        item.address,
                        amount,
                        block_daa,
                        coinbase,
                        if legacy { "credited" } else { "pending" },
                        daa,
                        if legacy { Some(daa) } else { None },
                    ],
                )?;
            }
        }

        for item in confirmed {
            let key_index = i64::from(item.output_index);
            let stored: Option<(String, i64, i64, i64, String)> = tx
                .query_row(
                    "SELECT address, amount_sompi, block_daa_score, is_coinbase, state
                     FROM deposits WHERE tx_id=?1 AND output_index=?2",
                    params![item.tx_id, key_index],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .optional()?;
            let Some((address, amount, block_daa, coinbase, _state)) = stored else {
                return Err(EngineError::Message(format!(
                    "confirmed outpoint {}:{} was never observed",
                    item.tx_id, item.output_index
                )));
            };
            if address != item.address
                || amount != to_i64(item.amount_sompi, "confirmed amount")?
                || block_daa != to_i64(item.block_daa_score, "confirmed block DAA")?
                || coinbase != i64::from(item.is_coinbase)
            {
                return Err(EngineError::Message(format!(
                    "confirmation facts changed for {}:{}",
                    item.tx_id, item.output_index
                )));
            }
            let changed = tx.execute(
                "UPDATE deposits SET state='credited', credited_daa=?3
                 WHERE tx_id=?1 AND output_index=?2 AND state='pending'",
                params![item.tx_id, key_index, daa],
            )?;
            if changed == 1 {
                insert_outbox(
                    &tx,
                    "credit",
                    &item.tx_id,
                    item.output_index,
                    u64::try_from(amount).map_err(|_| {
                        EngineError::Message("negative SQLite deposit amount".into())
                    })?,
                    &address,
                )?;
            }
        }

        for (tx_id, output_index) in disappeared {
            let state: Option<String> = tx
                .query_row(
                    "SELECT state FROM deposits WHERE tx_id=?1 AND output_index=?2",
                    params![tx_id, i64::from(*output_index)],
                    |row| row.get(0),
                )
                .optional()?;
            if state.as_deref() == Some("pending") {
                tx.execute(
                    "UPDATE deposits SET state='reversed', reversed_daa=?3,
                       reversal_reason='disappeared before confirmation'
                     WHERE tx_id=?1 AND output_index=?2",
                    params![tx_id, i64::from(*output_index), daa],
                )?;
                let facts: (i64, String) = tx.query_row(
                    "SELECT amount_sompi, address FROM deposits
                     WHERE tx_id=?1 AND output_index=?2",
                    params![tx_id, i64::from(*output_index)],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                insert_outbox(
                    &tx,
                    "reverse",
                    tx_id,
                    *output_index,
                    u64::try_from(facts.0).map_err(|_| {
                        EngineError::Message("negative SQLite deposit amount".into())
                    })?,
                    &facts.1,
                )?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Reverse a credited deposit only when an operator has explicit orphan/reorg evidence.
    pub fn reverse_credited_on_reorg(
        &mut self,
        virtual_daa: u64,
        tx_id: &str,
        output_index: u32,
        evidence: &str,
    ) -> Result<()> {
        let evidence = evidence.trim();
        if evidence.is_empty() {
            return Err(EngineError::Message(
                "credited reversal requires explicit reorg evidence".into(),
            ));
        }
        let daa = to_i64(virtual_daa, "virtual DAA")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let facts: Option<(String, i64, String)> = tx
            .query_row(
                "SELECT state, amount_sompi, address FROM deposits
                 WHERE tx_id=?1 AND output_index=?2",
                params![tx_id, i64::from(output_index)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((state, amount, address)) = facts else {
            return Err(EngineError::Message(format!(
                "cannot reverse unknown credited outpoint {tx_id}:{output_index}"
            )));
        };
        if state == "reversed" {
            tx.commit()?;
            return Ok(());
        }
        if state != "credited" {
            return Err(EngineError::Message(format!(
                "explicit reorg reversal requires credited state, found {state}"
            )));
        }
        tx.execute(
            "UPDATE deposits SET state='reversed', reversed_daa=?3, reversal_reason=?4
             WHERE tx_id=?1 AND output_index=?2 AND state='credited'",
            params![tx_id, i64::from(output_index), daa, evidence],
        )?;
        insert_outbox(
            &tx,
            "reverse",
            tx_id,
            output_index,
            u64::try_from(amount)
                .map_err(|_| EngineError::Message("negative SQLite deposit amount".into()))?,
            &address,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn pending_count(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM deposits WHERE state='pending'",
            [],
            |row| row.get(0),
        )?;
        usize::try_from(count)
            .map_err(|_| EngineError::Message("invalid pending deposit count".into()))
    }

    pub fn pending_outpoints(&self) -> Result<Vec<(String, u32)>> {
        let mut statement = self.conn.prepare(
            "SELECT tx_id, output_index FROM deposits
             WHERE state='pending' ORDER BY tx_id, output_index",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut outpoints = Vec::new();
        for row in rows {
            let (tx_id, output_index) = row?;
            outpoints.push((
                tx_id,
                u32::try_from(output_index)
                    .map_err(|_| EngineError::Message("invalid SQLite output index".into()))?,
            ));
        }
        Ok(outpoints)
    }

    pub fn unacknowledged_events(&self) -> Result<Vec<LedgerEvent>> {
        let mut statement = self.conn.prepare(
            "SELECT id, event_key, kind, tx_id, output_index, amount_sompi, address
             FROM deposit_outbox WHERE acknowledged=0 ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (id, event_key, kind, tx_id, output_index, amount, address) = row?;
            events.push(decode_event(
                id,
                event_key,
                kind,
                tx_id,
                output_index,
                amount,
                address,
            )?);
        }
        Ok(events)
    }

    pub fn unacknowledged_count(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM deposit_outbox WHERE acknowledged=0",
            [],
            |row| row.get(0),
        )?;
        usize::try_from(count)
            .map_err(|_| EngineError::Message("invalid unacknowledged event count".into()))
    }

    pub fn acknowledge_event(&mut self, id: i64) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE deposit_outbox
             SET acknowledged=1, lease_owner=NULL, lease_until=NULL, last_error=NULL
             WHERE id=?1 AND acknowledged=0 AND lease_owner IS NULL",
            [id],
        )?;
        if changed != 1 {
            return Err(EngineError::Message(format!(
                "outbox event {id} is missing or already acknowledged"
            )));
        }
        Ok(())
    }

    pub fn claim_next_event(
        &mut self,
        owner: &str,
        now_epoch_seconds: u64,
        lease_seconds: u64,
    ) -> Result<Option<LedgerEvent>> {
        validate_lease(owner, lease_seconds)?;
        let now = to_i64(now_epoch_seconds, "outbox lease time")?;
        let lease_until = to_i64(
            now_epoch_seconds
                .checked_add(lease_seconds)
                .ok_or_else(|| EngineError::Message("outbox lease time overflow".into()))?,
            "outbox lease expiry",
        )?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id: Option<i64> = tx
            .query_row(
                "SELECT id FROM deposit_outbox
                 WHERE acknowledged=0 AND (lease_until IS NULL OR lease_until<=?1)
                 ORDER BY id LIMIT 1",
                [now],
                |row| row.get(0),
            )
            .optional()?;
        let Some(id) = id else {
            tx.commit()?;
            return Ok(None);
        };
        let changed = tx.execute(
            "UPDATE deposit_outbox
             SET lease_owner=?2, lease_until=?3, attempts=attempts+1, last_error=NULL
             WHERE id=?1 AND acknowledged=0
               AND (lease_until IS NULL OR lease_until<=?4)",
            params![id, owner, lease_until, now],
        )?;
        if changed != 1 {
            return Err(EngineError::Message(
                "outbox event claim raced with another consumer".into(),
            ));
        }
        let row = tx.query_row(
            "SELECT id, event_key, kind, tx_id, output_index, amount_sompi, address
             FROM deposit_outbox WHERE id=?1 AND lease_owner=?2",
            params![id, owner],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )?;
        tx.commit()?;
        Ok(Some(decode_event(
            row.0, row.1, row.2, row.3, row.4, row.5, row.6,
        )?))
    }

    pub fn acknowledge_claim(&mut self, id: i64, owner: &str) -> Result<()> {
        validate_owner(owner)?;
        let changed = self.conn.execute(
            "UPDATE deposit_outbox
             SET acknowledged=1, lease_owner=NULL, lease_until=NULL, last_error=NULL
             WHERE id=?1 AND acknowledged=0 AND lease_owner=?2",
            params![id, owner],
        )?;
        if changed != 1 {
            return Err(EngineError::Message(format!(
                "outbox claim {id} is not owned by {owner}"
            )));
        }
        Ok(())
    }

    pub fn release_claim(&mut self, id: i64, owner: &str, error: &str) -> Result<()> {
        validate_owner(owner)?;
        let error = error.trim();
        if error.is_empty() || error.len() > 1_024 {
            return Err(EngineError::Message(
                "outbox delivery error must be 1-1024 bytes".into(),
            ));
        }
        let changed = self.conn.execute(
            "UPDATE deposit_outbox
             SET lease_owner=NULL, lease_until=NULL, last_error=?3
             WHERE id=?1 AND acknowledged=0 AND lease_owner=?2",
            params![id, owner, error],
        )?;
        if changed != 1 {
            return Err(EngineError::Message(format!(
                "outbox claim {id} is not owned by {owner}"
            )));
        }
        Ok(())
    }
}

fn decode_event(
    id: i64,
    event_key: String,
    kind: String,
    tx_id: String,
    output_index: i64,
    amount: i64,
    address: String,
) -> Result<LedgerEvent> {
    Ok(LedgerEvent {
        id,
        event_key,
        kind,
        tx_id,
        output_index: u32::try_from(output_index)
            .map_err(|_| EngineError::Message("invalid SQLite output index".into()))?,
        amount_sompi: u64::try_from(amount)
            .map_err(|_| EngineError::Message("invalid SQLite amount".into()))?,
        address,
    })
}

fn validate_owner(owner: &str) -> Result<()> {
    if owner.trim().is_empty() || owner.len() > 128 {
        return Err(EngineError::Message(
            "outbox lease owner must be 1-128 bytes".into(),
        ));
    }
    Ok(())
}

fn validate_lease(owner: &str, lease_seconds: u64) -> Result<()> {
    validate_owner(owner)?;
    if !(1..=3_600).contains(&lease_seconds) {
        return Err(EngineError::Message(
            "outbox lease must be 1-3600 seconds".into(),
        ));
    }
    Ok(())
}

fn migrate_v1_to_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "BEGIN IMMEDIATE;
         ALTER TABLE deposit_outbox ADD COLUMN lease_owner TEXT;
         ALTER TABLE deposit_outbox ADD COLUMN lease_until INTEGER;
         ALTER TABLE deposit_outbox ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE deposit_outbox ADD COLUMN last_error TEXT;
         UPDATE schema_meta SET value=2 WHERE key='version' AND value=1;
         COMMIT;",
    )?;
    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?)
}

fn require_columns(conn: &Connection, table: &str, required: &[&str]) -> Result<()> {
    let sql = format!("PRAGMA table_info({table})");
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    let columns = rows.collect::<std::result::Result<std::collections::HashSet<_>, _>>()?;
    for name in required {
        if !columns.contains(*name) {
            return Err(EngineError::Message(format!(
                "deposit ledger table {table} is missing required column {name}"
            )));
        }
    }
    Ok(())
}

fn insert_outbox(
    tx: &rusqlite::Transaction<'_>,
    kind: &str,
    tx_id: &str,
    output_index: u32,
    amount_sompi: u64,
    address: &str,
) -> Result<()> {
    let event_key = format!("{kind}:{tx_id}:{output_index}");
    tx.execute(
        "INSERT OR IGNORE INTO deposit_outbox(
           event_key, kind, tx_id, output_index, amount_sompi, address
         ) VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            event_key,
            kind,
            tx_id,
            i64::from(output_index),
            to_i64(amount_sompi, "deposit amount")?,
            address,
        ],
    )?;
    Ok(())
}

fn to_i64(value: u64, label: &str) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| EngineError::Message(format!("{label} exceeds SQLite INTEGER range")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed(daa: u64) -> UtxoAppearance {
        UtxoAppearance {
            tx_id: "tx".into(),
            output_index: 0,
            address: "kaspatest:abc".into(),
            amount_sompi: 10,
            block_daa_score: 100,
            virtual_daa: daa,
            is_coinbase: false,
        }
    }

    fn confirmed() -> ConfirmedDeposit {
        ConfirmedDeposit {
            tx_id: "tx".into(),
            output_index: 0,
            address: "kaspatest:abc".into(),
            amount_sompi: 10,
            block_daa_score: 100,
            confirmations: 60,
            is_coinbase: false,
        }
    }

    #[test]
    fn reopen_is_idempotent_and_outbox_is_durable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deposits.sqlite");
        {
            let mut ledger = DepositLedger::open(&path).unwrap();
            ledger
                .reconcile(160, &[observed(160)], &[confirmed()], &[])
                .unwrap();
            assert_eq!(ledger.unacknowledged_events().unwrap().len(), 1);
        }
        let mut ledger = DepositLedger::open(&path).unwrap();
        ledger
            .reconcile(161, &[observed(161)], &[confirmed()], &[])
            .unwrap();
        let events = ledger.unacknowledged_events().unwrap();
        assert_eq!(events.len(), 1);
        ledger.acknowledge_event(events[0].id).unwrap();
        assert!(ledger.unacknowledged_events().unwrap().is_empty());
    }

    #[test]
    fn outbox_claims_are_exclusive_expire_and_require_owner_ack() {
        let mut ledger = DepositLedger::open(":memory:").unwrap();
        ledger
            .reconcile(160, &[observed(160)], &[confirmed()], &[])
            .unwrap();
        let first = ledger
            .claim_next_event("worker-a", 100, 10)
            .unwrap()
            .unwrap();
        assert_eq!(first.event_key, "credit:tx:0");
        assert!(ledger
            .claim_next_event("worker-b", 109, 10)
            .unwrap()
            .is_none());
        let reclaimed = ledger
            .claim_next_event("worker-b", 110, 10)
            .unwrap()
            .unwrap();
        assert_eq!(reclaimed.id, first.id);
        assert!(ledger.acknowledge_claim(first.id, "worker-a").is_err());
        ledger
            .release_claim(first.id, "worker-b", "temporary HTTP 503")
            .unwrap();
        let retried = ledger
            .claim_next_event("worker-a", 111, 10)
            .unwrap()
            .unwrap();
        assert_eq!(retried.id, first.id);
        ledger.acknowledge_claim(first.id, "worker-a").unwrap();
        assert!(ledger.unacknowledged_events().unwrap().is_empty());
    }

    #[test]
    fn migrates_valid_v1_ledger_to_delivery_leases() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("v1.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_meta(key TEXT PRIMARY KEY, value INTEGER NOT NULL);
             INSERT INTO schema_meta VALUES ('version', 1);
             CREATE TABLE deposits(
               tx_id TEXT NOT NULL, output_index INTEGER NOT NULL, address TEXT NOT NULL,
               amount_sompi INTEGER NOT NULL, block_daa_score INTEGER NOT NULL,
               is_coinbase INTEGER NOT NULL, state TEXT NOT NULL,
               first_seen_daa INTEGER NOT NULL, last_seen_daa INTEGER NOT NULL,
               credited_daa INTEGER, reversed_daa INTEGER, reversal_reason TEXT,
               PRIMARY KEY(tx_id, output_index)
             );
             CREATE TABLE legacy_credits(
               tx_id TEXT NOT NULL, output_index INTEGER NOT NULL,
               PRIMARY KEY(tx_id, output_index)
             );
             CREATE TABLE deposit_outbox(
               id INTEGER PRIMARY KEY AUTOINCREMENT, event_key TEXT NOT NULL UNIQUE,
               kind TEXT NOT NULL, tx_id TEXT NOT NULL, output_index INTEGER NOT NULL,
               amount_sompi INTEGER NOT NULL, address TEXT NOT NULL,
               acknowledged INTEGER NOT NULL DEFAULT 0
             );",
        )
        .unwrap();
        drop(conn);

        let ledger = DepositLedger::open(&path).unwrap();
        let version: i64 = ledger
            .conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key='version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 2);
        require_columns(
            &ledger.conn,
            "deposit_outbox",
            &["lease_owner", "lease_until", "attempts", "last_error"],
        )
        .unwrap();
    }

    #[test]
    fn pending_disappears_but_credited_spend_does_not_reverse() {
        let mut pending = DepositLedger::open(":memory:").unwrap();
        pending.reconcile(100, &[observed(100)], &[], &[]).unwrap();
        pending
            .reconcile(101, &[], &[], &[("tx".into(), 0)])
            .unwrap();
        assert_eq!(pending.unacknowledged_events().unwrap()[0].kind, "reverse");

        let mut credited = DepositLedger::open(":memory:").unwrap();
        credited
            .reconcile(160, &[observed(160)], &[confirmed()], &[])
            .unwrap();
        credited
            .reconcile(161, &[], &[], &[("tx".into(), 0)])
            .unwrap();
        let events = credited.unacknowledged_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "credit");
    }

    #[test]
    fn immutable_fact_conflict_rolls_back() {
        let mut ledger = DepositLedger::open(":memory:").unwrap();
        ledger.reconcile(100, &[observed(100)], &[], &[]).unwrap();
        let mut changed = observed(101);
        changed.amount_sompi = 11;
        assert!(ledger.reconcile(101, &[changed], &[], &[]).is_err());
        assert_eq!(ledger.pending_count().unwrap(), 1);
        assert_eq!(ledger.pending_outpoints().unwrap(), vec![("tx".into(), 0)]);
    }

    #[test]
    fn confirmation_fact_conflict_does_not_credit() {
        let mut ledger = DepositLedger::open(":memory:").unwrap();
        ledger.reconcile(100, &[observed(100)], &[], &[]).unwrap();
        let mut changed = confirmed();
        changed.amount_sompi = 11;
        assert!(ledger.reconcile(160, &[], &[changed], &[]).is_err());
        assert_eq!(ledger.pending_count().unwrap(), 1);
        assert!(ledger.unacknowledged_events().unwrap().is_empty());
    }

    #[test]
    fn credited_reversal_requires_explicit_evidence_and_is_idempotent() {
        let mut ledger = DepositLedger::open(":memory:").unwrap();
        ledger
            .reconcile(160, &[observed(160)], &[confirmed()], &[])
            .unwrap();
        assert!(ledger.reverse_credited_on_reorg(170, "tx", 0, "").is_err());
        ledger
            .reverse_credited_on_reorg(170, "tx", 0, "orphan proof block=abc")
            .unwrap();
        ledger
            .reverse_credited_on_reorg(171, "tx", 0, "same orphan proof")
            .unwrap();
        let events = ledger.unacknowledged_events().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, "credit");
        assert_eq!(events[1].kind, "reverse");
    }

    #[test]
    fn refuses_unversioned_or_incompatible_existing_schema() {
        let directory = tempfile::tempdir().unwrap();
        let unversioned = directory.path().join("unversioned.sqlite");
        Connection::open(&unversioned)
            .unwrap()
            .execute("CREATE TABLE deposits(tx_id TEXT)", [])
            .unwrap();
        assert!(DepositLedger::open(&unversioned).is_err());

        let incompatible = directory.path().join("incompatible.sqlite");
        let conn = Connection::open(&incompatible).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_meta(key TEXT PRIMARY KEY, value INTEGER NOT NULL);
             INSERT INTO schema_meta VALUES ('version', 1);
             CREATE TABLE deposits(tx_id TEXT);",
        )
        .unwrap();
        drop(conn);
        assert!(DepositLedger::open(&incompatible).is_err());
    }
}
