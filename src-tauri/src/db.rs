//! Database bootstrap: connection pool, pragmas, and migrations.

use std::path::Path;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::error::RepoError;

/// Filename of the `SQLite` database inside the app-data directory.
const DB_FILENAME: &str = "talea.sqlite3";

/// The most memory `SQLite` may allocate, in bytes, across every connection in
/// the process (256 MiB). Restore attaches a downloaded file before anything in
/// it can be checked, and attaching makes `SQLite` load that file's statistics
/// tables, which a crafted file can size to gigabytes. Under this cap the attach
/// fails as out of memory, and the restore with it, instead of the device
/// killing the app. Real use, a large restore included, stays far below it.
/// It narrows that failure rather than removing it: while such an attach holds
/// most of the cap, a query on another connection whose row sqlx can't copy
/// aborts the app (sqlx `expect`s that copy to succeed). A single oversized
/// request fails without taking anything, so this needs a file sized to fill
/// the cap without overrunning it, and a query landing in that moment.
const HEAP_LIMIT_BYTES: &str = "268435456";

/// Opens (creating if needed) the database under `app_data_dir`, applies
/// pragmas, and runs all pending migrations.
///
/// `foreign_keys(true)` is essential: `SQLite` enforces foreign keys only when the
/// pragma is on, and it is per-connection, so it is set on every pooled
/// connection.
///
/// # Errors
///
/// Returns [`RepoError`] if the directory cannot be created, the database cannot
/// be opened, or a migration fails.
pub async fn init_pool(app_data_dir: &Path) -> Result<SqlitePool, RepoError> {
    std::fs::create_dir_all(app_data_dir)?;
    let db_path = app_data_dir.join(DB_FILENAME);

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        // WAL + Normal is the standard, safe pairing: durable against app
        // crashes; only an OS/power failure can lose the last transaction (which
        // WAL replay still recovers from without corruption). Don't "upgrade" to
        // Full without reason.
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        // Process-wide: setting it per connection only repeats it.
        .pragma("hard_heap_limit", HEAP_LIMIT_BYTES);

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;

    sqlx::migrate!().run(&pool).await?;

    Ok(pool)
}
