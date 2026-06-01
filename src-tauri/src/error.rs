//! Error types for the persistence layer and the command boundary.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use talea_core::DomainError;

use crate::webdav::WebDavError;

/// Internal error from the repository / database layer.
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    /// A `sqlx` query or connection error.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    /// A migration failed to apply.
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),

    /// Filesystem error preparing the database directory.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// An identifier supplied over IPC does not fit a `SQLite` `INTEGER` (it is
    /// outside `0..=i64::MAX`), so it cannot reference any row.
    #[error("identifier {0} is out of range")]
    InvalidId(u64),

    /// A stored row could not be reconstructed into a valid domain value. This
    /// means the database holds data we should never have written — corruption
    /// or external tampering — not bad user input.
    #[error("stored data is invalid (corrupt database): {0}")]
    Corrupt(String),
}

impl RepoError {
    /// Wraps a domain validation failure that occurred while reading stored
    /// data as a corruption error.
    pub(crate) fn corrupt(error: &DomainError) -> Self {
        Self::Corrupt(error.to_string())
    }
}

/// Error returned across the Tauri command boundary.
///
/// Serializes as `{ "code": "...", "message": "..." }` so the frontend can
/// branch on `code`. Internal details (SQL, file paths) are logged server-side
/// and never sent to the frontend.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// User input failed validation; `message` is safe to display.
    #[error("{0}")]
    Validation(String),

    /// The requested entity does not exist.
    #[error("not found")]
    NotFound,

    /// An internal database error occurred (details logged, not exposed).
    #[error("a database error occurred")]
    Database,

    /// The local data file is corrupt.
    #[error("the data file is corrupt")]
    Corrupt,

    /// A Nextcloud backup/restore operation failed; `message` is safe to display.
    #[error("{0}")]
    Backup(String),
}

impl CommandError {
    /// The stable machine-readable code the frontend branches on.
    fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation",
            Self::NotFound => "not_found",
            Self::Database => "database",
            Self::Corrupt => "corrupt",
            Self::Backup(_) => "backup",
        }
    }
}

impl From<WebDavError> for CommandError {
    /// `WebDAV` errors already carry user-safe, password-free messages.
    fn from(error: WebDavError) -> Self {
        Self::Backup(error.to_string())
    }
}

impl From<DomainError> for CommandError {
    /// A `DomainError` at the command layer comes from validating user input.
    fn from(error: DomainError) -> Self {
        Self::Validation(error.to_string())
    }
}

impl From<RepoError> for CommandError {
    fn from(error: RepoError) -> Self {
        match error {
            RepoError::InvalidId(raw) => {
                Self::Validation(format!("identifier {raw} is out of range"))
            }
            RepoError::Corrupt(inner) => {
                log::error!("corrupt database row: {inner}");
                Self::Corrupt
            }
            RepoError::Sqlx(inner) => {
                log::error!("database error: {inner}");
                Self::Database
            }
            RepoError::Migrate(inner) => {
                log::error!("migration error: {inner}");
                Self::Database
            }
            RepoError::Io(inner) => {
                log::error!("io error: {inner}");
                Self::Database
            }
        }
    }
}

impl Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("CommandError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}
