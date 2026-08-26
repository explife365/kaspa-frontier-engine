use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("network request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("unexpected JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("SQLite state error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("node reported network {found}, expected {expected}")]
    WrongNetwork { expected: String, found: String },
    #[error(
        "public Kaspa testnet is TN10; {found} is not used by this crate (TN11 retired; TN12 is not the public net)"
    )]
    UnsupportedTestnet { found: String },
    #[error("expected a kaspatest: address for TN10, got {0}")]
    NotTestnetAddress(String),
    #[error("REST endpoint must be https, got {0}")]
    InsecureTransport(String),
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;
