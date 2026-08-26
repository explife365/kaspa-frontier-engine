//! Durable, restart-safe TN10 withdrawal observation state.

use crate::error::{EngineError, Result};
use crate::exchange::{WithdrawalExpectation, WithdrawalUtxo};
use crate::network::is_valid_testnet_address;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::Path;
use std::time::Duration;

const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithdrawalState {
    Pending,
    Observed,
    Confirmed,
    Rejected,
}

impl WithdrawalState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Observed => "observed",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "observed" => Ok(Self::Observed),
            "confirmed" => Ok(Self::Confirmed),
            "rejected" => Ok(Self::Rejected),
            _ => Err(EngineError::Message(format!(
                "invalid SQLite withdrawal state {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithdrawalRecord {
    pub expected: WithdrawalExpectation,
    pub required_confirmations: u64,
    pub state: WithdrawalState,
    pub observed_block_daa: Option<u64>,
    pub last_checked_daa: u64,
    pub confirmed_daa: Option<u64>,
    pub rejected_daa: Option<u64>,
    pub rejection_reason: Option<String>,
}

pub struct WithdrawalLedger {
    connection: Connection,
}

impl WithdrawalLedger {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        let has_meta = table_exists(&connection, "withdrawal_schema_meta")?;
        if has_meta {
            let version: i64 = connection.query_row(
                "SELECT value FROM withdrawal_schema_meta WHERE key='version'",
                [],
                |row| row.get(0),
            )?;
            if version != SCHEMA_VERSION {
                return Err(EngineError::Message(format!(
                    "unsupported withdrawal ledger schema {version}; expected {SCHEMA_VERSION}"
                )));
            }
        } else if table_exists(&connection, "withdrawals")? {
            return Err(EngineError::Message(
                "unversioned withdrawal table exists".into(),
            ));
        }
        connection.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS withdrawal_schema_meta(
               key TEXT PRIMARY KEY,
               value INTEGER NOT NULL
             );
             INSERT INTO withdrawal_schema_meta(key, value) VALUES('version', 1)
               ON CONFLICT(key) DO NOTHING;
             CREATE TABLE IF NOT EXISTS withdrawals(
               tx_id TEXT NOT NULL,
               output_index INTEGER NOT NULL,
               dest TEXT NOT NULL,
               amount_sompi INTEGER NOT NULL,
               required_confirmations INTEGER NOT NULL CHECK(required_confirmations > 0),
               state TEXT NOT NULL CHECK(state IN ('pending','observed','confirmed','rejected')),
               observed_block_daa INTEGER,
               last_checked_daa INTEGER NOT NULL DEFAULT 0,
               confirmed_daa INTEGER,
               rejected_daa INTEGER,
               rejection_reason TEXT,
               PRIMARY KEY(tx_id, output_index)
             );
             COMMIT;",
        )?;
        require_columns(
            &connection,
            "withdrawals",
            &[
                "tx_id",
                "output_index",
                "dest",
                "amount_sompi",
                "required_confirmations",
                "state",
                "observed_block_daa",
                "last_checked_daa",
                "confirmed_daa",
                "rejected_daa",
                "rejection_reason",
            ],
        )?;
        Ok(Self { connection })
    }

    pub fn register(
        &mut self,
        expected: &WithdrawalExpectation,
        required_confirmations: u64,
    ) -> Result<WithdrawalRecord> {
        validate_expectation(expected)?;
        let required_confirmations = required_confirmations.max(1);
        let required = to_i64(required_confirmations, "withdrawal confirmations")?;
        let amount = to_i64(expected.amount_sompi, "withdrawal amount")?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = load_record(&transaction, &expected.tx_id, expected.output_index)?;
        if let Some(existing) = existing {
            if existing.expected != *expected
                || existing.required_confirmations != required_confirmations
            {
                return Err(EngineError::Message(format!(
                    "withdrawal facts conflict for {}:{}",
                    expected.tx_id, expected.output_index
                )));
            }
            transaction.commit()?;
            return Ok(existing);
        }
        transaction.execute(
            "INSERT INTO withdrawals(
               tx_id, output_index, dest, amount_sompi, required_confirmations, state
             ) VALUES(?1,?2,?3,?4,?5,'pending')",
            params![
                expected.tx_id,
                i64::from(expected.output_index),
                expected.dest,
                amount,
                required
            ],
        )?;
        let record = load_record(&transaction, &expected.tx_id, expected.output_index)?
            .ok_or_else(|| EngineError::Message("withdrawal insert disappeared".into()))?;
        transaction.commit()?;
        Ok(record)
    }

    pub fn observe(
        &mut self,
        expected: &WithdrawalExpectation,
        virtual_daa: u64,
        observed: &WithdrawalUtxo,
    ) -> Result<WithdrawalRecord> {
        validate_observation(expected, observed)?;
        let virtual_daa = to_i64(virtual_daa, "virtual DAA")?;
        let block_daa = to_i64(observed.block_daa_score, "withdrawal block DAA")?;
        if block_daa > virtual_daa {
            return Err(EngineError::Message(
                "withdrawal block DAA exceeds virtual DAA".into(),
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = load_record(&transaction, &expected.tx_id, expected.output_index)?
            .ok_or_else(|| EngineError::Message("withdrawal is not registered".into()))?;
        if record.state == WithdrawalState::Rejected {
            return Err(EngineError::Message(
                "rejected withdrawal requires operator review before re-observation".into(),
            ));
        }
        if record
            .observed_block_daa
            .is_some_and(|stored| stored != observed.block_daa_score)
        {
            return Err(EngineError::Message(format!(
                "withdrawal block DAA changed for {}:{}",
                expected.tx_id, expected.output_index
            )));
        }
        transaction.execute(
            "UPDATE withdrawals
             SET state=CASE WHEN state='pending' THEN 'observed' ELSE state END,
                 observed_block_daa=COALESCE(observed_block_daa, ?3),
                 last_checked_daa=?4
             WHERE tx_id=?1 AND output_index=?2",
            params![
                expected.tx_id,
                i64::from(expected.output_index),
                block_daa,
                virtual_daa
            ],
        )?;
        let record = load_record(&transaction, &expected.tx_id, expected.output_index)?
            .ok_or_else(|| EngineError::Message("withdrawal observation disappeared".into()))?;
        transaction.commit()?;
        Ok(record)
    }

    pub fn advance(
        &mut self,
        expected: &WithdrawalExpectation,
        virtual_daa: u64,
        transaction_accepted: Option<bool>,
    ) -> Result<WithdrawalRecord> {
        let virtual_daa_u64 = virtual_daa;
        let virtual_daa = to_i64(virtual_daa, "virtual DAA")?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = load_record(&transaction, &expected.tx_id, expected.output_index)?
            .ok_or_else(|| EngineError::Message("withdrawal is not registered".into()))?;
        if virtual_daa_u64 < record.last_checked_daa {
            return Err(EngineError::Message(
                "withdrawal virtual DAA moved backwards".into(),
            ));
        }
        if record.state != WithdrawalState::Rejected {
            if transaction_accepted == Some(false) && record.observed_block_daa.is_some() {
                transaction.execute(
                    "UPDATE withdrawals
                     SET state='rejected', rejected_daa=?3,
                         rejection_reason='observed transaction is no longer accepted',
                         last_checked_daa=?3
                     WHERE tx_id=?1 AND output_index=?2",
                    params![
                        expected.tx_id,
                        i64::from(expected.output_index),
                        virtual_daa
                    ],
                )?;
            } else if transaction_accepted == Some(true) {
                if let Some(block_daa) = record.observed_block_daa {
                    let confirmations = virtual_daa_u64.saturating_sub(block_daa);
                    if confirmations >= record.required_confirmations {
                        transaction.execute(
                            "UPDATE withdrawals
                             SET state='confirmed', confirmed_daa=COALESCE(confirmed_daa, ?3),
                                 last_checked_daa=?3
                             WHERE tx_id=?1 AND output_index=?2",
                            params![
                                expected.tx_id,
                                i64::from(expected.output_index),
                                virtual_daa
                            ],
                        )?;
                    } else {
                        update_last_checked(&transaction, expected, virtual_daa)?;
                    }
                } else {
                    update_last_checked(&transaction, expected, virtual_daa)?;
                }
            } else {
                update_last_checked(&transaction, expected, virtual_daa)?;
            }
        }
        let record = load_record(&transaction, &expected.tx_id, expected.output_index)?
            .ok_or_else(|| EngineError::Message("withdrawal update disappeared".into()))?;
        transaction.commit()?;
        Ok(record)
    }

    pub fn get(&self, expected: &WithdrawalExpectation) -> Result<Option<WithdrawalRecord>> {
        load_record(&self.connection, &expected.tx_id, expected.output_index)
    }
}

fn update_last_checked(
    connection: &Connection,
    expected: &WithdrawalExpectation,
    virtual_daa: i64,
) -> Result<()> {
    connection.execute(
        "UPDATE withdrawals SET last_checked_daa=?3
         WHERE tx_id=?1 AND output_index=?2",
        params![
            expected.tx_id,
            i64::from(expected.output_index),
            virtual_daa
        ],
    )?;
    Ok(())
}

fn load_record(
    connection: &Connection,
    tx_id: &str,
    output_index: u32,
) -> Result<Option<WithdrawalRecord>> {
    let row = connection
        .query_row(
            "SELECT dest, amount_sompi, required_confirmations, state,
                    observed_block_daa, last_checked_daa, confirmed_daa,
                    rejected_daa, rejection_reason
             FROM withdrawals WHERE tx_id=?1 AND output_index=?2",
            params![tx_id, i64::from(output_index)],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            },
        )
        .optional()?;
    let Some((
        dest,
        amount,
        required,
        state,
        observed,
        last_checked,
        confirmed,
        rejected,
        rejection_reason,
    )) = row
    else {
        return Ok(None);
    };
    Ok(Some(WithdrawalRecord {
        expected: WithdrawalExpectation {
            tx_id: tx_id.into(),
            dest,
            amount_sompi: from_i64(amount, "withdrawal amount")?,
            output_index,
        },
        required_confirmations: from_i64(required, "withdrawal confirmations")?,
        state: WithdrawalState::parse(&state)?,
        observed_block_daa: from_optional_i64(observed, "withdrawal block DAA")?,
        last_checked_daa: from_i64(last_checked, "withdrawal last checked DAA")?,
        confirmed_daa: from_optional_i64(confirmed, "withdrawal confirmed DAA")?,
        rejected_daa: from_optional_i64(rejected, "withdrawal rejected DAA")?,
        rejection_reason,
    }))
}

fn validate_expectation(expected: &WithdrawalExpectation) -> Result<()> {
    if expected.tx_id.len() != 64 || !expected.tx_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(EngineError::Message(
            "withdrawal txid must be exactly 32 bytes of hex".into(),
        ));
    }
    if !is_valid_testnet_address(&expected.dest) {
        return Err(EngineError::NotTestnetAddress(expected.dest.clone()));
    }
    if expected.amount_sompi == 0 {
        return Err(EngineError::Message(
            "withdrawal amount must be positive".into(),
        ));
    }
    Ok(())
}

fn validate_observation(expected: &WithdrawalExpectation, observed: &WithdrawalUtxo) -> Result<()> {
    if observed.tx_id != expected.tx_id
        || observed.output_index != expected.output_index
        || observed.address != expected.dest
        || observed.amount_sompi != expected.amount_sompi
    {
        return Err(EngineError::Message(format!(
            "withdrawal observation conflicts with expected {}:{}",
            expected.tx_id, expected.output_index
        )));
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
                "withdrawal ledger table {table} is missing required column {name}"
            )));
        }
    }
    Ok(())
}

fn to_i64(value: u64, label: &str) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| EngineError::Message(format!("{label} exceeds SQLite INTEGER range")))
}

fn from_i64(value: i64, label: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| EngineError::Message(format!("invalid SQLite {label}")))
}

fn from_optional_i64(value: Option<i64>, label: &str) -> Result<Option<u64>> {
    value.map(|value| from_i64(value, label)).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str = "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt";

    fn expected() -> WithdrawalExpectation {
        WithdrawalExpectation {
            tx_id: "a".repeat(64),
            dest: ADDRESS.into(),
            amount_sompi: 25_000_000,
            output_index: 1,
        }
    }

    fn observed() -> WithdrawalUtxo {
        WithdrawalUtxo {
            tx_id: "a".repeat(64),
            output_index: 1,
            amount_sompi: 25_000_000,
            block_daa_score: 1_000,
            address: ADDRESS.into(),
        }
    }

    #[test]
    fn restart_confirms_observed_then_spent_output() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("withdrawals.sqlite");
        {
            let mut ledger = WithdrawalLedger::open(&path).unwrap();
            ledger.register(&expected(), 60).unwrap();
            let record = ledger.observe(&expected(), 1_000, &observed()).unwrap();
            assert_eq!(record.state, WithdrawalState::Observed);
        }
        let mut restarted = WithdrawalLedger::open(&path).unwrap();
        let record = restarted.advance(&expected(), 1_060, Some(true)).unwrap();
        assert_eq!(record.state, WithdrawalState::Confirmed);
        assert_eq!(record.observed_block_daa, Some(1_000));
    }

    #[test]
    fn rejected_or_unobserved_transaction_never_confirms() {
        let mut ledger = WithdrawalLedger::open(":memory:").unwrap();
        ledger.register(&expected(), 60).unwrap();
        let pending = ledger.advance(&expected(), 2_000, Some(true)).unwrap();
        assert_eq!(pending.state, WithdrawalState::Pending);
        ledger.observe(&expected(), 2_001, &observed()).unwrap();
        let rejected = ledger.advance(&expected(), 2_002, Some(false)).unwrap();
        assert_eq!(rejected.state, WithdrawalState::Rejected);
        let still_rejected = ledger.advance(&expected(), 3_000, Some(true)).unwrap();
        assert_eq!(still_rejected.state, WithdrawalState::Rejected);
    }

    #[test]
    fn immutable_expectation_and_observation_conflicts_fail_closed() {
        let mut ledger = WithdrawalLedger::open(":memory:").unwrap();
        ledger.register(&expected(), 60).unwrap();
        let mut conflict = expected();
        conflict.amount_sompi += 1;
        assert!(ledger.register(&conflict, 60).is_err());
        let mut conflict = observed();
        conflict.address = "kaspatest:wrong".into();
        assert!(ledger.observe(&expected(), 1_000, &conflict).is_err());
        let mut conflict = observed();
        conflict.block_daa_score += 1;
        ledger.observe(&expected(), 1_001, &conflict).unwrap();
        assert!(ledger.observe(&expected(), 1_002, &observed()).is_err());
    }

    #[test]
    fn refuses_unversioned_or_incompatible_schema() {
        let directory = tempfile::tempdir().unwrap();
        let unversioned = directory.path().join("unversioned.sqlite");
        Connection::open(&unversioned)
            .unwrap()
            .execute("CREATE TABLE withdrawals(tx_id TEXT)", [])
            .unwrap();
        assert!(WithdrawalLedger::open(&unversioned).is_err());

        let incompatible = directory.path().join("incompatible.sqlite");
        let connection = Connection::open(&incompatible).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE withdrawal_schema_meta(
                   key TEXT PRIMARY KEY, value INTEGER NOT NULL
                 );
                 INSERT INTO withdrawal_schema_meta VALUES('version', 99);",
            )
            .unwrap();
        drop(connection);
        assert!(WithdrawalLedger::open(&incompatible).is_err());
    }
}
