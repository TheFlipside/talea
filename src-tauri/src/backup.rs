//! Nextcloud backup/restore: credential config, DB snapshot, in-place restore.
//!
//! The Nextcloud credentials live in a JSON file in the app-data directory —
//! deliberately **outside** the `SQLite` database, so the password is never part
//! of a backup we upload. The backup itself is a `VACUUM INTO` snapshot of the
//! database (a clean single file, no `WAL` sidecars). Restore replaces every table
//! in the live database in one transaction (no pool swap), and commits only once
//! the result checks out: a backup from a different schema version, or one
//! holding anything the app couldn't read, is refused and changes nothing.

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, Connection, SqlitePool};
use talea_core::{
    Account, AccountId, AccountKind, MAX_AMOUNT_SEGMENTS, MAX_LABEL_LEN, MAX_NOTE_LEN,
};
use tauri::async_runtime::Mutex;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::error::{CommandError, RepoError};
use crate::lock::lock;
use crate::repo;
use crate::webdav;

const CONFIG_FILE: &str = "nextcloud.json";
/// Scratch file for the `VACUUM INTO` snapshot we upload.
const SNAPSHOT_FILE: &str = "talea-backup-snapshot.sqlite3";
/// Scratch file for a downloaded backup we're about to restore.
const DOWNLOAD_FILE: &str = "talea-backup-download.sqlite3";
const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";
/// The longest app password stored. A real one is far shorter; this only
/// bounds what a crafted save can write.
const MAX_PASSWORD_BYTES: usize = 1_024;

/// A data table restore copies, and the most rows a backup may hold in it.
pub(crate) struct Table {
    pub(crate) name: &'static str,
    /// No ledger kept by hand comes near it. Past it, a crafted backup could
    /// exhaust memory while restore reads it back, or make every month view
    /// crawl: the ledger walks each month from an account's anchor, over all of
    /// the account's rows.
    pub(crate) max_rows: i64,
}

impl Table {
    const fn new(name: &'static str, max_rows: i64) -> Self {
        Self { name, max_rows }
    }
}

/// Data tables in dependency order, **parents before children**. Restore inserts
/// in this order and deletes in reverse, so the two halves can't drift apart —
/// there's a single list to update when a table is added.
pub(crate) const TABLES: &[Table] = &[
    Table::new("account", 200),
    // Up to 50 members (`MAX_SUMMARY_MEMBERS`) for every account.
    Table::new("account_member", 10_000),
    Table::new("category", 500),
    Table::new("budget", 2_000),
    // Up to 99 breakpoints (`MAX_AMOUNT_SEGMENTS`, less the base) per budget.
    Table::new("budget_amount", 198_000),
    Table::new("recurring_rule", 1_000),
    Table::new("entry", 250_000),
    // Up to 99 breakpoints per rule.
    Table::new("rule_amount", 99_000),
    Table::new("rule_skip", 100_000),
];

/// Stored Nextcloud connection. In app-data, never in the DB (so the password
/// is never uploaded inside a backup).
#[derive(Default, Clone, Serialize, Deserialize)]
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

/// Hand-written rather than derived, so the app password can never reach a log
/// or an error message through `{:?}`.
impl fmt::Debug for NextcloudConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let password = if self.password.is_empty() {
            ""
        } else {
            "[redacted]"
        };
        f.debug_struct("NextcloudConfig")
            .field("base_url", &self.base_url)
            .field("username", &self.username)
            .field("password", &password)
            .field("last_backup", &self.last_backup)
            .finish()
    }
}

impl NextcloudConfig {
    /// Whether enough is set to attempt a connection.
    pub fn is_configured(&self) -> bool {
        !self.base_url.is_empty() && !self.username.is_empty() && !self.password.is_empty()
    }

    /// Where a backup made with these settings goes.
    pub fn destination(&self) -> Destination {
        Destination {
            base_url: self.base_url.clone(),
            username: self.username.clone(),
        }
    }
}

/// Where backups go: a server address and the user on it, compared exactly as
/// stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Destination {
    base_url: String,
    username: String,
}

/// What saving the settings would do to where backups go.
#[derive(Debug)]
pub enum Change {
    /// Keeps the stored server and user.
    Keep,
    /// Clears both, disconnecting.
    Disconnect,
    /// Moves to a new server, which the user must confirm first.
    Move(NewServer),
}

/// A server that saving the settings would newly send backups to, as the user
/// is asked to confirm it. Only [`change`] makes one, so whatever the
/// confirmation shows has passed its checks.
#[derive(Debug, Clone)]
pub struct NewServer {
    /// The address as typed, trimmed: what is stored.
    base_url: String,
    /// The address as requests use it, and the confirmation shows it.
    address: String,
    username: String,
}

impl NewServer {
    /// The address, as [`webdav::server_address`] shows it.
    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn username(&self) -> &str {
        &self.username
    }
}

/// The most characters a new username may have: Nextcloud's own limit for a
/// user id, so the confirmation can show any username whole.
const MAX_USERNAME_CHARS: usize = 64;

/// Whether `c` may stand in a new username: Nextcloud allows a user id only
/// ASCII letters, digits, spaces and `_.@-'`, and its `WebDAV` paths name the
/// user by id. That leaves out everything the confirmation couldn't show as it
/// is: a line break faking a line of the dialog's own, a direction override or
/// an invisible character disguising the name, a letter from another script
/// passing for a Latin one, and a `%`, which the Linux dialog reads as
/// formatting (see [`webdav::server_address`]).
fn in_nextcloud_user_id(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '.' | '@' | '-' | '\'')
}

/// What saving `base_url` and `username` would do, against the `stored`
/// destination: keep it, if they are the stored ones; disconnect, if they are
/// both blank; or move to a new server, which must then be named outright,
/// address and user both, in a form the confirmation can show whole and as it
/// is.
///
/// # Errors
/// [`CommandError::Backup`] if a new server's address isn't one
/// [`webdav::server_address`] accepts, or its username is blank, too long, or
/// holds a character no Nextcloud user id does.
pub fn change(
    stored: &Destination,
    base_url: &str,
    username: &str,
) -> Result<Change, CommandError> {
    let (base_url, username) = (base_url.trim(), username.trim());
    if base_url.is_empty() && username.is_empty() {
        return Ok(Change::Disconnect);
    }
    if base_url == stored.base_url && username == stored.username {
        return Ok(Change::Keep);
    }
    let address = webdav::server_address(base_url)?;
    let valid_username = !username.is_empty()
        && username.chars().count() <= MAX_USERNAME_CHARS
        && username.chars().all(in_nextcloud_user_id)
        // `.` and `..` are no user ids: the URL would drop them as segments.
        && username.chars().any(|c| c != '.');
    if !valid_username {
        return Err(CommandError::Backup(
            "Enter your Nextcloud user ID: up to 64 letters, digits, spaces and _ . @ - ' only."
                .into(),
        ));
    }
    Ok(Change::Move(NewServer {
        base_url: base_url.to_owned(),
        address,
        username: username.to_owned(),
    }))
}

/// Saves the backup settings (see [`write_settings`]), but asks `confirm`
/// first whenever they name a new server (see [`change`]); returns whether
/// they were saved. The question is decided against the settings stored when
/// the save began, and the save goes through only if they are still stored
/// once it is answered.
///
/// # Errors
/// [`CommandError::Backup`] if the password is longer than any app password,
/// a new server isn't one that can be confirmed, `confirm` fails, the settings
/// changed while it was asked, or they can't be saved.
pub async fn save_settings<F, Fut>(
    app_data_dir: &Path,
    base_url: &str,
    username: &str,
    password: &str,
    confirm: F,
) -> Result<bool, CommandError>
where
    F: FnOnce(NewServer) -> Fut,
    Fut: Future<Output = Result<bool, CommandError>>,
{
    // Refused before the user is asked to confirm a save that would fail.
    check_password(password)?;
    let stored = load_config(app_data_dir).destination();
    let change = change(&stored, base_url, username)?;
    if let Change::Move(server) = &change {
        if !confirm(server.clone()).await? {
            return Ok(false);
        }
    }
    write_settings(app_data_dir, &stored, &change, password)?;
    Ok(true)
}

/// Refuses a password longer than any app password. A save for the stored
/// server asks nothing, so this bounds what a script can write.
fn check_password(password: &str) -> Result<(), CommandError> {
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(CommandError::Backup(
            "That app password is too long.".into(),
        ));
    }
    Ok(())
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

/// Writes a file readable only by its owner where the OS supports it, so neither
/// the stored app password nor a database copy on its way to or from the server
/// is world-readable (Unix/Android default umask is `0644`). On other platforms
/// the app-data directory's ACLs already restrict it.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        // `mode` applies only to a file `open` creates; one already there (left
        // by an older release, say) keeps its own until set here, while still
        // empty.
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(bytes)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)
    }
}

/// Guards `nextcloud.json`: every read and every read-modify-write holds it, so
/// a save can't land between another's read and write and be undone, nor a
/// read see a write half done. It guards no data of its own, so poisoning is
/// ignored.
static CONFIG: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Loads the config, or a default (everything empty) if absent/unreadable. An
/// unreadable file thus reads as nothing configured, and saving the settings
/// afresh, a server confirmed like any new one, is the way back.
pub fn load_config(app_data_dir: &Path) -> NextcloudConfig {
    let _config = lock(&CONFIG);
    read_config(app_data_dir)
}

/// [`load_config`], for a caller already holding [`CONFIG`].
fn read_config(app_data_dir: &Path) -> NextcloudConfig {
    try_read_config(app_data_dir).unwrap_or_default()
}

/// The stored config, if it can be read: none is stored, or it's unreadable,
/// otherwise.
fn try_read_config(app_data_dir: &Path) -> Option<NextcloudConfig> {
    std::fs::read(config_path(app_data_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn save_config(app_data_dir: &Path, config: &NextcloudConfig) -> Result<(), CommandError> {
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|_| CommandError::Backup("Couldn't save the backup settings.".into()))?;
    write_private(&config_path(app_data_dir), &bytes).map_err(|err| {
        log::error!("nextcloud config write failed: {err}");
        CommandError::Backup("Couldn't save the backup settings.".into())
    })
}

/// Writes the `change` [`save_settings`] has checked, and confirmed if it
/// moves to a new server. An empty `password` keeps the existing one, so the
/// frontend never needs to echo it back, but only for the same server and
/// user. The app password belongs to them: kept for a changed address, it
/// would be sent to whatever host answers there now, so a crafted call could
/// point the address at its own server and collect the password by testing
/// the connection. So for a new server, an empty password drops the stored one
/// instead, leaving the backup unconfigured until the new server's password is
/// entered. Disconnecting clears everything, password and all, and the
/// last-backup time goes with any change of server: that backup lives on the
/// previous one.
///
/// The comparison behind `change` is deliberately exact (after trimming): the
/// same server written differently only costs re-entering the password,
/// whereas a lenient match that went wrong would hand it to another host.
///
/// Writes only `if_still` the stored destination: `change` was decided
/// against it, and once the settings have moved on, that decision no longer
/// holds. The check and the write hold [`CONFIG`] throughout, which is what
/// keeps another save out from between them; the tests interleave saves only
/// one after another, so they don't pin that.
///
/// # Errors
/// [`CommandError::Backup`] if the stored destination is no longer `if_still`,
/// or the settings can't be saved.
fn write_settings(
    app_data_dir: &Path,
    if_still: &Destination,
    change: &Change,
    password: &str,
) -> Result<(), CommandError> {
    let _config = lock(&CONFIG);
    let mut config = read_config(app_data_dir);
    if config.destination() != *if_still {
        return Err(CommandError::Backup(
            "The backup settings changed meanwhile. Check them and save again.".into(),
        ));
    }
    match change {
        Change::Keep => {
            if !password.is_empty() {
                password.clone_into(&mut config.password);
            }
        }
        Change::Disconnect => config = NextcloudConfig::default(),
        Change::Move(server) => {
            server.base_url.clone_into(&mut config.base_url);
            server.username.clone_into(&mut config.username);
            // An empty one drops the stored password, as above.
            password.clone_into(&mut config.password);
            config.last_backup = None;
        }
    }
    save_config(app_data_dir, &config)
}

/// Backs up with `upload`, given the settings as stored now, then records the
/// backup against the server those settings named (see [`mark_backed_up`]),
/// whatever they name by the time it finishes. Returns the backup's time, or
/// `None` if they moved to another server meanwhile.
///
/// # Errors
/// Whatever `upload` fails with, or [`CommandError::Backup`] if the time can't
/// be written down.
pub async fn back_up<F, Fut>(app_data_dir: &Path, upload: F) -> Result<Option<String>, CommandError>
where
    F: FnOnce(NextcloudConfig) -> Fut,
    Fut: Future<Output = Result<(), CommandError>>,
{
    let config = load_config(app_data_dir);
    let uploaded_to = config.destination();
    upload(config).await?;
    mark_backed_up(app_data_dir, &uploaded_to)
}

/// Records the time of a backup just uploaded to `uploaded_to`, and returns
/// it, or `None` if the settings have since moved to another server: this
/// backup isn't on theirs, so they keep their own last one.
///
/// # Errors
/// [`CommandError::Backup`] if the settings can't be read, or the time can't
/// be written down.
pub fn mark_backed_up(
    app_data_dir: &Path,
    uploaded_to: &Destination,
) -> Result<Option<String>, CommandError> {
    let stamp = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|_| CommandError::Backup("Couldn't record the backup time.".into()))?;
    let _config = lock(&CONFIG);
    // Settings that can't be read haven't moved anywhere: that is a failure
    // to record the time, not a backup made for other settings.
    let mut config = try_read_config(app_data_dir).ok_or_else(|| {
        CommandError::Backup("Backed up, but couldn't record the backup time.".into())
    })?;
    if config.destination() != *uploaded_to {
        return Ok(None);
    }
    config.last_backup = Some(stamp.clone());
    save_config(app_data_dir, &config)?;
    Ok(Some(stamp))
}

/// Produces a clean snapshot of the database as a byte vector (via
/// `VACUUM INTO`, so no `-wal`/`-shm` sidecars are involved).
pub async fn snapshot(pool: &SqlitePool, app_data_dir: &Path) -> Result<Vec<u8>, CommandError> {
    let _scratch = SCRATCH.lock().await;
    let path = app_data_dir.join(SNAPSHOT_FILE);
    let result = snapshot_into(pool, &path).await;
    remove_scratch(&path);
    result
}

async fn snapshot_into(pool: &SqlitePool, path: &Path) -> Result<Vec<u8>, CommandError> {
    let unprepared = |err: std::io::Error| {
        log::error!("preparing the snapshot failed: {err}");
        CommandError::Backup("Couldn't prepare the backup.".into())
    };
    // `VACUUM INTO` fills an empty file it finds rather than creating its own,
    // so create it first, private (it holds every figure), clearing whatever a
    // crash left behind.
    remove_scratch(path);
    write_private(path, &[]).map_err(unprepared)?;
    let target = sql_path(path)?;
    // Safe: `target` is an app-controlled path with quotes escaped, not user input.
    sqlx::query(AssertSqlSafe(format!("VACUUM INTO '{target}'")))
        .execute(pool)
        .await
        .map_err(RepoError::from)?;
    std::fs::read(path).map_err(unprepared)
}

/// Replaces all local data with the contents of `bytes` (a downloaded backup),
/// atomically. Rejects anything that isn't a same-version Talea database, or
/// that holds data the app couldn't read or keep up with (see [`replace_all`]).
pub async fn restore(
    pool: &SqlitePool,
    app_data_dir: &Path,
    bytes: &[u8],
) -> Result<(), CommandError> {
    restore_within(pool, app_data_dir, bytes, PRE_CHECK_BUDGET).await
}

/// [`restore`], giving the file `budget` to prove itself ours (see
/// [`PRE_CHECK_BUDGET`]); tests shorten it.
pub(crate) async fn restore_within(
    pool: &SqlitePool,
    app_data_dir: &Path,
    bytes: &[u8],
    budget: Duration,
) -> Result<(), CommandError> {
    if !bytes.starts_with(SQLITE_MAGIC) {
        return Err(not_a_backup());
    }
    let _scratch = SCRATCH.lock().await;
    let path = app_data_dir.join(DOWNLOAD_FILE);
    // A crash can leave a previous download's sidecars, which the next attach
    // would open beside this file.
    remove_scratch(&path);
    let result = match write_private(&path, bytes) {
        Ok(()) => restore_from_file(pool, &path, budget).await,
        Err(err) => {
            log::error!("writing restore snapshot failed: {err}");
            Err(CommandError::Backup(
                "Couldn't read the downloaded backup.".into(),
            ))
        }
    };
    remove_scratch(&path);
    result
}

/// Deletes a scratch file once it's done with. A failure is only logged: the
/// next run empties the same file before using it.
fn remove_scratch(path: &Path) {
    // With any sidecar `SQLite` left beside it: a file in WAL mode gets two.
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        let file = PathBuf::from(name);
        match std::fs::remove_file(&file) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
                log::warn!("removing {} failed: {err}", file.display());
            }
            _ => {}
        }
    }
}

async fn restore_from_file(
    pool: &SqlitePool,
    path: &Path,
    budget: Duration,
) -> Result<(), CommandError> {
    let mut conn = pool.acquire().await.map_err(RepoError::from)?;
    // This connection ends up with the backup attached, and a restore that
    // failed or was cancelled can't be trusted to have let go of it: retire
    // the connection rather than return it to the pool, even if this future is
    // dropped midway.
    conn.close_on_drop();
    let result = attach_and_replace(&mut conn, path, budget).await;
    // Close it here rather than in the background, as dropping it would, so
    // the scratch file is let go of before it is removed.
    if let Err(err) = conn.close().await {
        log::warn!("closing the restore connection failed: {err}");
    }
    result
}

async fn attach_and_replace(
    conn: &mut sqlx::SqliteConnection,
    path: &Path,
    budget: Duration,
) -> Result<(), CommandError> {
    let deadline = Instant::now() + budget;
    conn.lock_handle()
        .await
        .map_err(RepoError::from)?
        .set_progress_handler(PROGRESS_STEPS, move || Instant::now() < deadline);
    // ATTACH must run outside a transaction. Safe: `attach` is an app-controlled
    // path with quotes escaped. A failed attach is the file's doing in practice:
    // not a database at all, or one demanding more memory or time than it may.
    let attach = sql_path(path)?;
    sqlx::query(AssertSqlSafe(format!(
        "ATTACH DATABASE '{attach}' AS backup"
    )))
    .execute(&mut *conn)
    .await
    .map_err(|err| refuse(format_args!("attaching it: {err}"), not_a_backup()))?;
    replace_all(conn).await
}

/// The refusal for anything that isn't a well-formed Talea backup.
fn not_a_backup() -> CommandError {
    CommandError::Backup("That file isn't a Talea backup.".into())
}

/// The refusal for a well-formed backup holding data the app can't read.
fn unreadable_backup() -> CommandError {
    CommandError::Backup("This backup holds data Talea can't read, so nothing was restored.".into())
}

/// The refusal for a backup holding more rows than [`Table::max_rows`] allows,
/// or text longer than [`max_text_bytes`].
fn oversized_backup() -> CommandError {
    CommandError::Backup(
        "This backup holds more data than Talea can handle, so nothing was restored.".into(),
    )
}

/// Refuses a restore with `refusal`, logging `why` for diagnosis: the user only
/// ever sees `refusal`.
fn refuse(why: fmt::Arguments<'_>, refusal: CommandError) -> CommandError {
    log::warn!("restore refused: {why}");
    refusal
}

/// The primary `SQLite` result codes restore tells apart. sqlx reports the
/// extended code, whose low byte is the primary one.
const SQLITE_NOMEM: i32 = 7;
const SQLITE_INTERRUPT: i32 = 9;
const SQLITE_CORRUPT: i32 = 11;
const SQLITE_CONSTRAINT: i32 = 19;
const SQLITE_TOOBIG: i32 = 18;
const SQLITE_NOTADB: i32 = 26;

/// The primary `SQLite` result code of a database error, if `err` is one.
fn primary_code(err: &sqlx::Error) -> Option<i32> {
    let code = err.as_database_error()?.code()?.parse::<i32>().ok()?;
    Some(code & 0xff)
}

/// Whether a failure reading the downloaded file is the file's own doing: it
/// is damaged or not a database, holds a value that won't decode or is too big,
/// kept `SQLite` busy past [`PRE_CHECK_BUDGET`], or asked it for more memory
/// than its cap (a real backup needs a sliver of it). Anything else, an I/O
/// error say, is this device's.
fn file_at_fault(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::ColumnDecode { .. } | sqlx::Error::Decode(_)
    ) || matches!(
        primary_code(err),
        Some(SQLITE_NOMEM | SQLITE_INTERRUPT | SQLITE_CORRUPT | SQLITE_TOOBIG | SQLITE_NOTADB)
    )
}

/// A failure reading the downloaded file: refused with `refusal` when it's the
/// file's doing, otherwise this device's own error, so a passing I/O fault
/// doesn't condemn a good backup.
fn reading_failure(
    what: fmt::Arguments<'_>,
    err: sqlx::Error,
    refusal: fn() -> CommandError,
) -> CommandError {
    if file_at_fault(&err) {
        refuse(format_args!("{what}: {err}"), refusal())
    } else {
        RepoError::from(err).into()
    }
}

/// Keeps each fixed scratch file to one user at a time, whoever calls. (The
/// commands also run backups and restores one at a time from end to end, but
/// these functions don't rely on that.)
static SCRATCH: Mutex<()> = Mutex::const_new(());

/// How long a downloaded file may keep `SQLite` busy before it is known to be
/// ours: being attached, then read for its version, its tables compared and
/// checked, and its rows counted. Attaching alone runs a query over the file's
/// statistics tables, which it controls, so a crafted file could otherwise
/// stall restore for as long as it liked. A real backup needs a sliver of this.
const PRE_CHECK_BUDGET: Duration = Duration::from_secs(60);

/// `SQLite` virtual-machine steps between checks of [`PRE_CHECK_BUDGET`].
const PROGRESS_STEPS: i32 = 1_000;

/// The highest row id or AUTOINCREMENT counter a backup may hold. Ids cross to
/// the frontend as JSON numbers, so they must stay within JavaScript's safe
/// integers (up to 2^53 − 1). New ids count up from the highest restored one,
/// and stopping at 2^52 leaves 2^52 more before one could alias another.
const MAX_RESTORED_ID: i64 = 1 << 52;

/// Replaces the live data with the attached backup's, in one transaction that
/// commits only once the result checks out:
///
/// 1. The backup's schema version must be this app's, or it's refused as
///    another version's. That check comes first, so an older backup gets that
///    message rather than a shape refusal.
/// 2. Every table restore reads must be this app's exactly, and intact
///    ([`check_tables`]), before anything is read from one; then no table may
///    hold more rows than it allows ([`check_volume`]). Until here, the file has
///    [`PRE_CHECK_BUDGET`] to get through.
/// 3. Copy, then read it all back the way the app will
///    ([`validate_restored`]) while it can still roll back.
async fn replace_all(conn: &mut sqlx::SqliteConnection) -> Result<(), CommandError> {
    // Restoring mismatched columns would corrupt data, so same version only.
    // `query_scalar` decodes with `try_get`, which fails on a value of the wrong
    // type where `get` would panic, and release builds abort on panic.
    let live: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM main._sqlx_migrations")
            .fetch_one(&mut *conn)
            .await
            .map_err(RepoError::from)?;
    if backup_version(conn).await? != live {
        return Err(CommandError::Backup(
            "This backup is from a different app version. Update Talea to the same version on \
             both devices, then try again."
                .into(),
        ));
    }
    check_tables(conn).await?;
    check_volume(conn).await?;
    // The file is ours, and its size bounded: the copy needn't race the clock.
    conn.lock_handle()
        .await
        .map_err(RepoError::from)?
        .remove_progress_handler();

    // Use sqlx's managed transaction so the connection can never be returned to
    // the pool mid-transaction: on any early return (including a failed commit)
    // the dropped `tx` rolls back.
    let mut tx = conn.begin().await.map_err(RepoError::from)?;
    copy_tables(&mut tx).await?;
    validate_restored(&mut tx).await?;
    tx.commit().await.map_err(RepoError::from)?;
    Ok(())
}

/// The backup's schema version: the newest migration it records. It's read only
/// once `_sqlx_migrations` is known to be a plain table with a stored `version`,
/// for the reason given at [`check_tables`], and one step at a time, so nothing
/// about the table is asked before its kind is known. Its `CREATE` statement
/// isn't compared outright the way ours are: the table is sqlx's, and a sqlx
/// release could reword it without our schema changing.
async fn backup_version(conn: &mut sqlx::SqliteConnection) -> Result<i64, CommandError> {
    // A view's statement starts `CREATE VIEW`, a virtual table's `CREATE
    // VIRTUAL TABLE`: reading either runs code the file supplies.
    let plain: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM backup.sqlite_master
                        WHERE name = '_sqlx_migrations' AND sql LIKE 'CREATE TABLE %')",
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|err| reading_failure(format_args!("finding its migrations"), err, not_a_backup))?;
    if !plain {
        return Err(refuse(
            format_args!("no plain migrations table"),
            not_a_backup(),
        ));
    }
    let stored: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pragma_table_xinfo('_sqlx_migrations', 'backup')
                        WHERE name = 'version' AND hidden = 0)",
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|err| reading_failure(format_args!("reading its migrations"), err, not_a_backup))?;
    if !stored {
        return Err(refuse(format_args!("a computed version"), not_a_backup()));
    }
    // `NOT INDEXED`, here and wherever the file's tables are read: nothing
    // checks that a backup's indexes agree with its tables.
    sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM backup._sqlx_migrations NOT INDEXED")
        .fetch_one(&mut *conn)
        .await
        .map_err(|err| reading_failure(format_args!("reading its version"), err, not_a_backup))
}

/// Refuses a backup unless each table restore reads is exactly this app's and
/// intact, before anything is read from one. The comparison is of the stored
/// `CREATE` statements, which evaluates nothing from the file; reading a view,
/// a virtual table or a generated column would run SQL the file supplies,
/// enough to synthesise rows or allocate without bound. Nothing else in the
/// file is ever read, so nothing else is compared. The integrity check is
/// scoped to these tables for the same reason: over the whole file it would
/// evaluate every other table's `CHECK` constraints too.
async fn check_tables(conn: &mut sqlx::SqliteConnection) -> Result<(), CommandError> {
    for name in TABLES
        .iter()
        .map(|table| table.name)
        .chain(["sqlite_sequence"])
    {
        let ours: bool = sqlx::query_scalar(
            "SELECT COALESCE(
                 (SELECT sql FROM main.sqlite_master WHERE type = 'table' AND name = ?1)
                 = (SELECT sql FROM backup.sqlite_master WHERE type = 'table' AND name = ?1),
                 FALSE
             )",
        )
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .map_err(|err| reading_failure(format_args!("comparing {name}"), err, not_a_backup))?;
        if !ours {
            return Err(refuse(
                format_args!("{name} isn't this app's"),
                not_a_backup(),
            ));
        }
        // Safe: table names come only from `TABLES` and the literal above. A
        // damaged table can fail the check outright rather than report itself.
        let integrity: Vec<String> =
            sqlx::query_scalar(AssertSqlSafe(format!("PRAGMA backup.quick_check({name})")))
                .fetch_all(&mut *conn)
                .await
                .map_err(|err| {
                    reading_failure(format_args!("checking {name}"), err, unreadable_backup)
                })?;
        if integrity != ["ok"] {
            return Err(refuse(
                format_args!("{name} is damaged"),
                unreadable_backup(),
            ));
        }
    }
    Ok(())
}

/// The longest text `column` may hold, in bytes: the domain's limit for it, at
/// UTF-8's four bytes a character. Notes run to [`MAX_NOTE_LEN`] characters,
/// labels, names and icons to [`MAX_LABEL_LEN`]; the rest (money, dates,
/// kinds, codes) never pass a few dozen bytes.
fn max_text_bytes(column: &str) -> i64 {
    let chars = match column {
        "note" => MAX_NOTE_LEN,
        "name" | "label" | "icon" | "icon_value" => MAX_LABEL_LEN,
        _ => 16,
    };
    i64::try_from(chars).map_or(i64::MAX, |chars| 4 * chars)
}

/// Refuses a backup holding more than the app could keep up with, before any
/// of it is copied or read back: more rows in a table than [`Table::max_rows`]
/// allows, more amount breakpoints under one rule or budget than the domain
/// allows, text longer than [`max_text_bytes`], or counters other than one per
/// table. Each is read from the table itself (`NOT INDEXED`): nothing checks
/// that the file's indexes agree with its tables, so an index could hide rows
/// from a count. Counting stops one past the cap, so a vast table costs no more
/// to refuse than an allowed one to pass. A cell longer than any the app writes
/// would pass validation where the domain trims it (an amount padded with
/// spaces, say), and then cost its whole size on every read.
async fn check_volume(conn: &mut sqlx::SqliteConnection) -> Result<(), CommandError> {
    for table in TABLES {
        let name = table.name;
        // Safe: table names come only from the hardcoded `TABLES` constant.
        let rows: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM (SELECT 1 FROM backup.{name} NOT INDEXED LIMIT ?)"
        )))
        .bind(table.max_rows + 1)
        .fetch_one(&mut *conn)
        .await
        .map_err(|err| reading_failure(format_args!("counting {name}"), err, unreadable_backup))?;
        if rows > table.max_rows {
            return Err(refuse(
                format_args!("{name} has over {} rows", table.max_rows),
                oversized_backup(),
            ));
        }
        // `octet_length` reads a cell's size without loading the cell. Safe:
        // column names come from this app's own schema, caps from constants.
        let too_long = declared_columns(conn, name)
            .await?
            .iter()
            .filter(|(_, kind)| kind == "TEXT")
            .map(|(column, _)| format!("octet_length(\"{column}\") > {}", max_text_bytes(column)))
            .collect::<Vec<_>>();
        if too_long.is_empty() {
            continue;
        }
        let long: bool = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT EXISTS (SELECT 1 FROM backup.{name} NOT INDEXED WHERE {})",
            too_long.join(" OR ")
        )))
        .fetch_one(&mut *conn)
        .await
        .map_err(|err| reading_failure(format_args!("measuring {name}"), err, unreadable_backup))?;
        if long {
            return Err(refuse(
                format_args!("{name} holds text longer than any the app writes"),
                oversized_backup(),
            ));
        }
    }
    check_breakpoints(conn).await?;
    check_counters(conn).await
}

/// Refuses a rule or budget with more amount breakpoints than its history may
/// hold (`MAX_AMOUNT_SEGMENTS`, the base amount among them). Each is read with
/// all of its breakpoints at once, so the table's own cap isn't enough: it
/// would let every breakpoint sit under one rule.
async fn check_breakpoints(conn: &mut sqlx::SqliteConnection) -> Result<(), CommandError> {
    let most = i64::try_from(MAX_AMOUNT_SEGMENTS).map_or(i64::MAX, |segments| segments - 1);
    for (table, parent) in [("rule_amount", "rule_id"), ("budget_amount", "budget_id")] {
        // Safe: table and column names are the literals above.
        let crowded: bool = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT EXISTS (SELECT 1 FROM backup.{table} NOT INDEXED
                            GROUP BY {parent} HAVING COUNT(*) > ?)"
        )))
        .bind(most)
        .fetch_one(&mut *conn)
        .await
        .map_err(|err| reading_failure(format_args!("counting {table}"), err, unreadable_backup))?;
        if crowded {
            return Err(refuse(
                format_args!("a {parent} with over {most} breakpoints"),
                oversized_backup(),
            ));
        }
    }
    Ok(())
}

/// Refuses AUTOINCREMENT counters other than at most one for each table in
/// [`TABLES`]. Every insert into such a table looks its counter up by scanning
/// them all, so millions of stray ones would slow every later insert.
async fn check_counters(conn: &mut sqlx::SqliteConnection) -> Result<(), CommandError> {
    let limit = i64::try_from(TABLES.len()).unwrap_or(i64::MAX);
    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (SELECT 1 FROM backup.sqlite_sequence NOT INDEXED LIMIT ?)",
    )
    .bind(limit + 1)
    .fetch_one(&mut *conn)
    .await
    .map_err(|err| reading_failure(format_args!("counting counters"), err, unreadable_backup))?;
    if rows > limit {
        return Err(refuse(
            format_args!("more counters than tables"),
            unreadable_backup(),
        ));
    }
    // Few enough to look at. Each counter must be an integer within
    // [`MAX_RESTORED_ID`] (one at `i64::MAX` would make every later insert
    // fail), under a short name. The column is untyped, and the type and size
    // checks come first, so a vast value isn't loaded to be compared.
    let malformed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM backup.sqlite_sequence NOT INDEXED
                        WHERE typeof(name) <> 'text' OR octet_length(name) > 64
                           OR typeof(seq) <> 'integer' OR seq NOT BETWEEN 0 AND ?)",
    )
    .bind(MAX_RESTORED_ID)
    .fetch_one(&mut *conn)
    .await
    .map_err(|err| reading_failure(format_args!("reading counters"), err, unreadable_backup))?;
    if malformed {
        return Err(refuse(
            format_args!("counters out of range"),
            unreadable_backup(),
        ));
    }
    // Safe: the list holds only the `TABLES` names.
    let names = TABLES
        .iter()
        .map(|table| format!("'{}'", table.name))
        .collect::<Vec<_>>()
        .join(", ");
    let stray: bool = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT COUNT(DISTINCT name) <> COUNT(*) OR COALESCE(SUM(name NOT IN ({names})), 0) > 0
         FROM backup.sqlite_sequence NOT INDEXED"
    )))
    .fetch_one(&mut *conn)
    .await
    .map_err(|err| reading_failure(format_args!("reading counters"), err, unreadable_backup))?;
    if stray {
        return Err(refuse(
            format_args!("counters other than one per table"),
            unreadable_backup(),
        ));
    }
    Ok(())
}

async fn copy_tables(conn: &mut sqlx::SqliteConnection) -> Result<(), CommandError> {
    // Defer FK checks to commit, so delete/insert order can't trip a constraint.
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *conn)
        .await
        .map_err(RepoError::from)?;
    // Safe: table names come only from the hardcoded `TABLES` constant, and
    // column names from this app's own schema. Delete children-first (reverse
    // dependency order), insert parents-first (forward).
    for table in TABLES.iter().rev() {
        sqlx::query(AssertSqlSafe(format!("DELETE FROM main.{}", table.name)))
            .execute(&mut *conn)
            .await
            .map_err(RepoError::from)?;
    }
    for table in TABLES {
        let name = table.name;
        let columns = column_list(conn, name).await?;
        sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO main.{name} ({columns}) SELECT {columns} FROM backup.{name} NOT INDEXED"
        )))
        .execute(&mut *conn)
        .await
        .map_err(|err| copy_failure(name, err))?;
    }
    // Keep AUTOINCREMENT counters in step so new ids don't collide with restored
    // rows.
    sqlx::query("DELETE FROM main.sqlite_sequence")
        .execute(&mut *conn)
        .await
        .map_err(RepoError::from)?;
    sqlx::query(
        "INSERT INTO main.sqlite_sequence (name, seq)
         SELECT name, seq FROM backup.sqlite_sequence NOT INDEXED",
    )
    .execute(&mut *conn)
    .await
    .map_err(RepoError::from)?;
    Ok(())
}

/// `table`'s columns as this app's schema declares them, in order, each with
/// its declared type in upper case.
async fn declared_columns(
    conn: &mut sqlx::SqliteConnection,
    table: &str,
) -> Result<Vec<(String, String)>, CommandError> {
    sqlx::query_as("SELECT name, upper(type) FROM pragma_table_info(?, 'main') ORDER BY cid")
        .bind(table)
        .fetch_all(&mut *conn)
        .await
        .map_err(|err| reading_failure(format_args!("listing {table}"), err, unreadable_backup))
}

/// `table`'s columns, quoted and comma-separated. The copy names them rather
/// than selecting `*`: given `*`, `SQLite` copies a table without foreign keys
/// record by record, unparsed, so fields a crafted backup tacked on past the
/// declared columns would be committed unseen, and turn up in whatever column
/// a later migration adds.
async fn column_list(
    conn: &mut sqlx::SqliteConnection,
    table: &str,
) -> Result<String, CommandError> {
    Ok(declared_columns(conn, table)
        .await?
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", "))
}

/// Classifies a failed copy. A row the live schema refuses (any constraint it
/// enforces on insert, a duplicate key say) or a damaged page is the backup's
/// fault, and refuses it. Anything else, a full disk say, is this device's,
/// and keeps its own error rather than blaming the file.
fn copy_failure(table: &str, err: sqlx::Error) -> CommandError {
    if file_at_fault(&err) || primary_code(&err) == Some(SQLITE_CONSTRAINT) {
        refuse(format_args!("copying {table}: {err}"), unreadable_backup())
    } else {
        RepoError::from(err).into()
    }
}

/// Reads the restored data back the way the app will, inside the restore's
/// transaction. A backup holding anything the app couldn't load or keep
/// working with (hand-edited, tampered with, or written by a bug) is refused
/// here, before it replaces the local data, rather than committed and left
/// broken with no way back short of wiping the app. That covers ids out of
/// range, rows the validating constructors reject, broken summary accounts, and
/// orphaned rows.
async fn validate_restored(conn: &mut sqlx::SqliteConnection) -> Result<(), CommandError> {
    check_ids(conn).await?;
    repo::category::list(&mut *conn)
        .await
        .map_err(|err| read_back_failure("categories", err))?;
    let accounts = repo::account::list_on(&mut *conn)
        .await
        .map_err(|err| read_back_failure("accounts", err))?;
    let by_id: HashMap<AccountId, &Account> = accounts
        .iter()
        .map(|account| (account.id(), account))
        .collect();
    for account in &accounts {
        account
            .check_members(|id| by_id.get(&id).copied())
            .map_err(|err| {
                refuse(
                    format_args!("a summary account: {err}"),
                    unreadable_backup(),
                )
            })?;
        let data = repo::ledger::records(&mut *conn, account.clone())
            .await
            .map_err(|err| read_back_failure("an account's records", err))?;
        // A summary's figures come from its members, so records of its own
        // would sit in the database unseen.
        let owns_records =
            !(data.entries.is_empty() && data.rules.is_empty() && data.budgets.is_empty());
        if account.kind() == AccountKind::Summary && owns_records {
            return Err(refuse(
                format_args!("a summary account owns records"),
                unreadable_backup(),
            ));
        }
    }
    // Deferred foreign keys would only fail at commit, as an opaque database
    // error: check them now, so an orphaned row gets the same plain refusal. One
    // is enough to refuse, so stop at the first.
    let orphan = sqlx::query("PRAGMA main.foreign_key_check")
        .fetch_optional(&mut *conn)
        .await
        .map_err(RepoError::from)?;
    if orphan.is_some() {
        return Err(refuse(format_args!("orphaned rows"), unreadable_backup()));
    }
    Ok(())
}

/// Refuses ids the frontend couldn't tell apart: every table's highest rowid,
/// and every AUTOINCREMENT counter, must be at most [`MAX_RESTORED_ID`]. A
/// table's rowid is its `id` where it has one; the copy numbers the rest
/// afresh, since it names its columns. Ids below 1 are refused as the rows are
/// read back. The counters were held to this before the copy
/// ([`check_counters`]); checking them again as committed costs nothing (there
/// are a handful) and doesn't rest on both reads seeing the same rows.
async fn check_ids(conn: &mut sqlx::SqliteConnection) -> Result<(), CommandError> {
    for table in TABLES {
        let name = table.name;
        // Safe: table names come only from the hardcoded `TABLES` constant. A
        // lone MAX seeks straight to the last row.
        let highest: Option<i64> =
            sqlx::query_scalar(AssertSqlSafe(format!("SELECT MAX(rowid) FROM main.{name}")))
                .fetch_one(&mut *conn)
                .await
                .map_err(RepoError::from)?;
        if highest.is_some_and(|id| id > MAX_RESTORED_ID) {
            return Err(refuse(
                format_args!("{name} ids out of range"),
                unreadable_backup(),
            ));
        }
    }
    let bad_counter: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM main.sqlite_sequence
                        WHERE typeof(seq) <> 'integer' OR seq NOT BETWEEN 0 AND ?)",
    )
    .bind(MAX_RESTORED_ID)
    .fetch_one(&mut *conn)
    .await
    .map_err(RepoError::from)?;
    if bad_counter {
        return Err(refuse(
            format_args!("counters out of range"),
            unreadable_backup(),
        ));
    }
    Ok(())
}

/// Classifies a failure reading the restored rows back. A row that doesn't
/// decode or validate is the backup's fault, and refuses it. Anything else, an
/// I/O error say, is this device's, and keeps its own error.
fn read_back_failure(what: &str, err: RepoError) -> CommandError {
    match err {
        RepoError::Corrupt(_)
        | RepoError::Sqlx(sqlx::Error::ColumnDecode { .. } | sqlx::Error::Decode(_)) => {
            refuse(format_args!("reading {what}: {err}"), unreadable_backup())
        }
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use sqlx::{Connection, SqliteConnection};

    use super::{copy_failure, not_a_backup, reading_failure};
    use crate::error::CommandError;

    #[tokio::test]
    async fn a_failed_copy_blames_the_backup_only_for_a_refused_row() {
        let mut conn = SqliteConnection::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql(
            "PRAGMA foreign_keys = ON; \
             CREATE TABLE p (id INTEGER PRIMARY KEY); \
             CREATE TABLE t (id INTEGER PRIMARY KEY, \
                             n INTEGER NOT NULL CHECK (n > 0), \
                             p INTEGER REFERENCES p (id)); \
             CREATE TABLE s (n INTEGER) STRICT; \
             INSERT INTO t VALUES (1, 1, NULL)",
        )
        .execute(&mut conn)
        .await
        .unwrap();
        // One row per constraint SQLite enforces on insert.
        for refused_row in [
            "INSERT INTO t VALUES (1, 1, NULL)",
            "INSERT INTO t VALUES (2, NULL, NULL)",
            "INSERT INTO t VALUES (3, 0, NULL)",
            "INSERT INTO t VALUES (4, 1, 9)",
            "INSERT INTO s VALUES ('not a number')",
        ] {
            let err = sqlx::query(refused_row)
                .execute(&mut conn)
                .await
                .unwrap_err();
            assert!(
                matches!(copy_failure("t", err), CommandError::Backup(message)
                    if message.contains("can't read")),
                "{refused_row}"
            );
        }
        // Any other failure is this device's, and keeps its own error.
        let err = sqlx::query("INSERT INTO missing VALUES (1)")
            .execute(&mut conn)
            .await
            .unwrap_err();
        assert!(matches!(copy_failure("t", err), CommandError::Database));
    }

    #[tokio::test]
    async fn a_failed_read_of_the_file_blames_it_only_for_its_own_faults() {
        let mut conn = SqliteConnection::connect("sqlite::memory:").await.unwrap();
        let blamed = |err| {
            matches!(
                reading_failure(format_args!("reading"), err, not_a_backup),
                CommandError::Backup(_)
            )
        };
        // A value that won't decode is the file's.
        let err = sqlx::query_scalar::<_, i64>("SELECT 'not a number'")
            .fetch_one(&mut conn)
            .await
            .unwrap_err();
        assert!(blamed(err));
        // So is keeping SQLite busy past its time.
        conn.lock_handle()
            .await
            .unwrap()
            .set_progress_handler(1, || false);
        let err = sqlx::query("SELECT 1")
            .fetch_one(&mut conn)
            .await
            .unwrap_err();
        assert!(blamed(err));
        conn.lock_handle().await.unwrap().remove_progress_handler();
        // And asking for more than a value may be, or than the heap cap (the
        // app's own, set here as `db.rs` sets it) allows.
        let err = sqlx::query("SELECT zeroblob(2000000000)")
            .fetch_one(&mut conn)
            .await
            .unwrap_err();
        assert!(blamed(err));
        sqlx::query("PRAGMA hard_heap_limit = 268435456")
            .execute(&mut conn)
            .await
            .unwrap();
        let err = sqlx::query("SELECT randomblob(300000000)")
            .fetch_one(&mut conn)
            .await
            .unwrap_err();
        assert!(blamed(err));
        // Anything else is not.
        let err = sqlx::query("SELECT * FROM missing")
            .fetch_one(&mut conn)
            .await
            .unwrap_err();
        assert!(!blamed(err));
    }

    #[test]
    fn each_text_cap_fits_the_longest_value_the_app_writes() {
        let bytes = |chars: usize| 4 * i64::try_from(chars).unwrap();
        assert_eq!(
            super::max_text_bytes("note"),
            bytes(talea_core::MAX_NOTE_LEN)
        );
        for column in ["name", "label", "icon", "icon_value"] {
            assert_eq!(
                super::max_text_bytes(column),
                bytes(talea_core::MAX_LABEL_LEN)
            );
        }
        // An amount at `Decimal`'s widest, or a date at its widest, fits easily.
        let widest_amount = "-7922816251426433759354395.0335";
        assert!(i64::try_from(widest_amount.len()).unwrap() <= super::max_text_bytes("amount"));
        assert!(16 <= super::max_text_bytes("occurrence_date"));
    }

    #[cfg(unix)]
    #[test]
    fn a_private_write_makes_a_file_already_there_private_too() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scratch.sqlite3");
        std::fs::write(&path, b"stale").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        super::write_private(&path, b"fresh").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"fresh");
    }
}
