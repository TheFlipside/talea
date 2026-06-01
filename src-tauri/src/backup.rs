//! Nextcloud backup/restore: credential config, DB snapshot, in-place restore.
//!
//! The Nextcloud credentials live in a JSON file in the app-data directory —
//! deliberately **outside** the `SQLite` database, so the password is never part
//! of a backup we upload. The backup itself is a `VACUUM INTO` snapshot of the
//! database (a clean single file, no `WAL` sidecars). Restore replaces every table
//! in the live database in one transaction (no pool swap), guarded so a backup
//! from a different schema version is refused rather than risking a mismatch.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, Connection, Row, SqlitePool};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::error::{CommandError, RepoError};

const CONFIG_FILE: &str = "nextcloud.json";
/// Scratch file for the `VACUUM INTO` snapshot we upload.
const SNAPSHOT_FILE: &str = "talea-backup-snapshot.sqlite3";
/// Scratch file for a downloaded backup we're about to restore.
const DOWNLOAD_FILE: &str = "talea-backup-download.sqlite3";
const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

/// Data tables in dependency order, **parents before children**. Restore inserts
/// in this order and deletes in reverse, so the two halves can't drift apart —
/// there's a single list to update when a table is added.
const TABLES: &[&str] = &[
    "account",
    "category",
    "recurring_rule",
    "entry",
    "rule_amount",
    "rule_skip",
];

/// Stored Nextcloud connection. In app-data, never in the DB (so the password
/// is never uploaded inside a backup).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextcloudConfig {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    /// RFC-3339 timestamp of the last successful backup, if any.
    #[serde(default)]
    pub last_backup: Option<String>,
}

impl NextcloudConfig {
    /// Whether enough is set to attempt a connection.
    pub fn is_configured(&self) -> bool {
        !self.base_url.is_empty() && !self.username.is_empty() && !self.password.is_empty()
    }
}

/// What the frontend may see — never the password.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextcloudConfigView {
    pub base_url: String,
    pub username: String,
    pub configured: bool,
    pub last_backup: Option<String>,
}

impl From<&NextcloudConfig> for NextcloudConfigView {
    fn from(config: &NextcloudConfig) -> Self {
        Self {
            base_url: config.base_url.clone(),
            username: config.username.clone(),
            configured: config.is_configured(),
            last_backup: config.last_backup.clone(),
        }
    }
}

fn config_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(CONFIG_FILE)
}

/// Renders `path` for embedding in a single-quoted SQL string literal (quotes
/// doubled). Fails on a non-UTF-8 path rather than lossily mangling it into a
/// string that no longer names the real file.
fn sql_path(path: &Path) -> Result<String, CommandError> {
    path.to_str()
        .map(|text| text.replace('\'', "''"))
        .ok_or_else(|| CommandError::Backup("App storage path isn't usable.".into()))
}

/// Writes a file readable only by its owner where the OS supports it, so the
/// stored app password isn't world-readable (Unix/Android default umask is
/// `0644`). On other platforms the app-data directory's ACLs already restrict it.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)
    }
}

/// Loads the config, or a default (everything empty) if absent/unreadable.
pub fn load_config(app_data_dir: &Path) -> NextcloudConfig {
    std::fs::read(config_path(app_data_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_config(app_data_dir: &Path, config: &NextcloudConfig) -> Result<(), CommandError> {
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|_| CommandError::Backup("Couldn't save the backup settings.".into()))?;
    write_private(&config_path(app_data_dir), &bytes).map_err(|err| {
        log::error!("nextcloud config write failed: {err}");
        CommandError::Backup("Couldn't save the backup settings.".into())
    })
}

/// Updates the stored credentials. An empty `password` keeps the existing one
/// (so the frontend never needs to echo it back).
pub fn set_credentials(
    app_data_dir: &Path,
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<(), CommandError> {
    let mut config = load_config(app_data_dir);
    base_url.trim().clone_into(&mut config.base_url);
    username.trim().clone_into(&mut config.username);
    if !password.is_empty() {
        password.clone_into(&mut config.password);
    }
    save_config(app_data_dir, &config)
}

/// Records the time of a successful backup.
pub fn mark_backed_up(app_data_dir: &Path) -> Result<String, CommandError> {
    let stamp = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|_| CommandError::Backup("Couldn't record the backup time.".into()))?;
    let mut config = load_config(app_data_dir);
    config.last_backup = Some(stamp.clone());
    save_config(app_data_dir, &config)?;
    Ok(stamp)
}

/// Produces a clean snapshot of the database as a byte vector (via
/// `VACUUM INTO`, so no `-wal`/`-shm` sidecars are involved).
pub async fn snapshot(pool: &SqlitePool, app_data_dir: &Path) -> Result<Vec<u8>, CommandError> {
    let path = app_data_dir.join(SNAPSHOT_FILE);
    let _ = std::fs::remove_file(&path); // clear any stale snapshot
    let target = sql_path(&path)?;
    // Safe: `target` is an app-controlled path with quotes escaped, not user input.
    sqlx::query(AssertSqlSafe(format!("VACUUM INTO '{target}'")))
        .execute(pool)
        .await
        .map_err(RepoError::from)?;
    let bytes = std::fs::read(&path).map_err(|err| {
        log::error!("reading snapshot failed: {err}");
        CommandError::Backup("Couldn't prepare the backup.".into())
    });
    let _ = std::fs::remove_file(&path);
    bytes
}

/// Replaces all local data with the contents of `bytes` (a downloaded backup),
/// atomically. Rejects anything that isn't a same-version Talea database.
pub async fn restore(
    pool: &SqlitePool,
    app_data_dir: &Path,
    bytes: &[u8],
) -> Result<(), CommandError> {
    if !bytes.starts_with(SQLITE_MAGIC) {
        return Err(CommandError::Backup(
            "That file isn't a Talea backup.".into(),
        ));
    }
    let path = app_data_dir.join(DOWNLOAD_FILE);
    std::fs::write(&path, bytes).map_err(|err| {
        log::error!("writing restore snapshot failed: {err}");
        CommandError::Backup("Couldn't read the downloaded backup.".into())
    })?;
    let result = restore_from_file(pool, &path).await;
    let _ = std::fs::remove_file(&path);
    result
}

async fn restore_from_file(pool: &SqlitePool, path: &Path) -> Result<(), CommandError> {
    let mut conn = pool.acquire().await.map_err(RepoError::from)?;
    // ATTACH must run outside a transaction.
    let attach = sql_path(path)?;
    // Safe: `attach` is an app-controlled path with quotes escaped.
    sqlx::query(AssertSqlSafe(format!(
        "ATTACH DATABASE '{attach}' AS backup"
    )))
    .execute(&mut *conn)
    .await
    .map_err(|_| CommandError::Backup("That file isn't a Talea backup.".into()))?;

    let result = replace_all(&mut conn).await;
    let _ = sqlx::query("DETACH DATABASE backup")
        .execute(&mut *conn)
        .await;
    result
}

async fn replace_all(conn: &mut sqlx::SqliteConnection) -> Result<(), CommandError> {
    // Refuse a backup whose schema version differs from this app's — restoring
    // mismatched columns would corrupt data. Same-version only.
    let live: i64 = sqlx::query("SELECT COALESCE(MAX(version), 0) FROM main._sqlx_migrations")
        .fetch_one(&mut *conn)
        .await
        .map_err(RepoError::from)?
        .get(0);
    let backup: i64 = sqlx::query("SELECT COALESCE(MAX(version), 0) FROM backup._sqlx_migrations")
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| CommandError::Backup("That file isn't a Talea backup.".into()))?
        .get(0);
    if live != backup {
        return Err(CommandError::Backup(
            "This backup is from a different app version. Update Talea to the same version on \
             both devices, then try again."
                .into(),
        ));
    }

    // Use sqlx's managed transaction so the connection can never be returned to
    // the pool mid-transaction: on any early return (including a failed commit)
    // the dropped `tx` rolls back.
    let mut tx = conn.begin().await.map_err(RepoError::from)?;
    copy_tables(&mut tx).await?;
    tx.commit().await.map_err(RepoError::from)?;
    Ok(())
}

async fn copy_tables(conn: &mut sqlx::SqliteConnection) -> Result<(), CommandError> {
    // Defer FK checks to commit, so delete/insert order can't trip a constraint.
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *conn)
        .await
        .map_err(RepoError::from)?;
    // Safe: table names come only from the hardcoded `TABLES` constant. Delete
    // children-first (reverse dependency order), insert parents-first (forward).
    for table in TABLES.iter().rev() {
        sqlx::query(AssertSqlSafe(format!("DELETE FROM main.{table}")))
            .execute(&mut *conn)
            .await
            .map_err(RepoError::from)?;
    }
    for table in TABLES {
        sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO main.{table} SELECT * FROM backup.{table}"
        )))
        .execute(&mut *conn)
        .await
        .map_err(RepoError::from)?;
    }
    // Keep AUTOINCREMENT counters in step so new ids don't collide with restored
    // rows.
    sqlx::query("DELETE FROM main.sqlite_sequence")
        .execute(&mut *conn)
        .await
        .map_err(RepoError::from)?;
    sqlx::query("INSERT INTO main.sqlite_sequence SELECT * FROM backup.sqlite_sequence")
        .execute(&mut *conn)
        .await
        .map_err(RepoError::from)?;
    Ok(())
}
