//! Tauri command surface: the only bridge between the frontend and the domain.
//!
//! Commands are thin — they validate input, call the repository, and (for
//! summaries) the pure `talea_core` ledger functions. No domain logic lives
//! here. All take the shared [`SqlitePool`] from Tauri state and return a
//! [`CommandError`] on failure.

use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

use talea_core::{
    expenses_by_category as core_expenses_by_category, month_summary as core_month_summary,
    summaries_for_range as core_summaries_for_range, Account, AccountId, Category, CategoryExpense,
    CategoryId, Entry, EntryId, Month, MonthSummary, RecurringRule, RecurringRuleId, VirtualEntry,
};

use crate::backup::{self, NextcloudConfig, NextcloudConfigView};
use crate::dto::{NewAccount, NewCategory, NewEntry, NewRule, OccurrenceRef};
use crate::error::{CommandError, RepoError};
use crate::repo;
use crate::webdav::WebDav;

/// Upper bound on the number of months a single range query may span (100
/// years), so a crafted `from..=to` can't force a huge loop/allocation.
const MAX_RANGE_MONTHS: i64 = 1200;

/// Maps a repository "row not found" (no rows affected) to [`CommandError::NotFound`].
fn require_found(updated: bool) -> Result<(), CommandError> {
    if updated {
        Ok(())
    } else {
        Err(CommandError::NotFound)
    }
}

// ---- Account ----------------------------------------------------------------

/// Creates an account.
///
/// # Errors
/// [`CommandError::Validation`] on invalid input; [`CommandError::Database`] on
/// a database error.
#[tauri::command]
pub async fn create_account(
    state: State<'_, SqlitePool>,
    account: NewAccount,
) -> Result<Account, CommandError> {
    let draft = account.build()?;
    Ok(repo::account::insert(state.inner(), &draft).await?)
}

/// Lists all accounts.
///
/// # Errors
/// [`CommandError::Database`] / [`CommandError::Corrupt`] on a database error.
#[tauri::command]
pub async fn list_accounts(state: State<'_, SqlitePool>) -> Result<Vec<Account>, CommandError> {
    Ok(repo::account::list(state.inner()).await?)
}

/// Updates an account.
///
/// # Errors
/// [`CommandError::NotFound`] if it does not exist; [`CommandError::Database`].
#[tauri::command]
pub async fn update_account(
    state: State<'_, SqlitePool>,
    account: Account,
) -> Result<Account, CommandError> {
    require_found(repo::account::update(state.inner(), &account).await?)?;
    Ok(account)
}

/// Deletes an account (cascading its entries and rules).
///
/// # Errors
/// [`CommandError::NotFound`] if it does not exist; [`CommandError::Database`].
#[tauri::command]
pub async fn delete_account(
    state: State<'_, SqlitePool>,
    id: AccountId,
) -> Result<(), CommandError> {
    require_found(repo::account::delete(state.inner(), id).await?)
}

// ---- Category ---------------------------------------------------------------

/// Creates a category.
///
/// # Errors
/// [`CommandError::Validation`] on invalid input; [`CommandError::Database`].
#[tauri::command]
pub async fn create_category(
    state: State<'_, SqlitePool>,
    category: NewCategory,
) -> Result<Category, CommandError> {
    let draft = category.build()?;
    Ok(repo::category::insert(state.inner(), &draft).await?)
}

/// Lists all categories.
///
/// # Errors
/// [`CommandError::Database`] / [`CommandError::Corrupt`] on a database error.
#[tauri::command]
pub async fn list_categories(state: State<'_, SqlitePool>) -> Result<Vec<Category>, CommandError> {
    Ok(repo::category::list(state.inner()).await?)
}

/// Updates a category.
///
/// # Errors
/// [`CommandError::NotFound`] if it does not exist; [`CommandError::Database`].
#[tauri::command]
pub async fn update_category(
    state: State<'_, SqlitePool>,
    category: Category,
) -> Result<Category, CommandError> {
    require_found(repo::category::update(state.inner(), &category).await?)?;
    Ok(category)
}

/// Deletes a category (entries/rules keep their data, losing the category).
///
/// # Errors
/// [`CommandError::NotFound`] if it does not exist; [`CommandError::Database`].
#[tauri::command]
pub async fn delete_category(
    state: State<'_, SqlitePool>,
    id: CategoryId,
) -> Result<(), CommandError> {
    require_found(repo::category::delete(state.inner(), id).await?)
}

// ---- Entry ------------------------------------------------------------------

/// Creates an entry.
///
/// # Errors
/// [`CommandError::Validation`] on invalid input; [`CommandError::Database`].
#[tauri::command]
pub async fn create_entry(
    state: State<'_, SqlitePool>,
    entry: NewEntry,
) -> Result<Entry, CommandError> {
    let draft = entry.build()?;
    Ok(repo::entry::insert(state.inner(), &draft).await?)
}

/// Lists all entries for an account.
///
/// # Errors
/// [`CommandError::Database`] / [`CommandError::Corrupt`] on a database error.
#[tauri::command]
pub async fn list_entries(
    state: State<'_, SqlitePool>,
    account_id: AccountId,
) -> Result<Vec<Entry>, CommandError> {
    Ok(repo::entry::for_account(state.inner(), account_id).await?)
}

/// Updates an entry.
///
/// # Errors
/// [`CommandError::NotFound`] if it does not exist; [`CommandError::Database`].
#[tauri::command]
pub async fn update_entry(
    state: State<'_, SqlitePool>,
    entry: Entry,
) -> Result<Entry, CommandError> {
    require_found(repo::entry::update(state.inner(), &entry).await?)?;
    Ok(entry)
}

/// Deletes an entry.
///
/// # Errors
/// [`CommandError::NotFound`] if it does not exist; [`CommandError::Database`].
#[tauri::command]
pub async fn delete_entry(state: State<'_, SqlitePool>, id: EntryId) -> Result<(), CommandError> {
    require_found(repo::entry::delete(state.inner(), id).await?)
}

/// Records a transfer between two accounts as a pair of mirrored entries: the
/// given `entry` on its own account, plus its opposite (income↔expense, same
/// amount and date, same note, no category) on `counter_account_id`. Both are
/// written in one transaction; the two accounts must share a currency (there is
/// no conversion). The entries are independent thereafter.
///
/// # Errors
/// [`CommandError::Validation`] on invalid input or a currency mismatch;
/// [`CommandError::NotFound`] if either account is missing;
/// [`CommandError::Database`] on a database error.
#[tauri::command]
pub async fn create_transfer(
    state: State<'_, SqlitePool>,
    entry: NewEntry,
    counter_account_id: AccountId,
) -> Result<(Entry, Entry), CommandError> {
    transfer(state.inner(), entry, counter_account_id).await
}

/// The transfer logic, separated from the Tauri command so it's directly
/// testable against a pool (the command is a thin wrapper).
pub(crate) async fn transfer(
    pool: &SqlitePool,
    entry: NewEntry,
    counter_account_id: AccountId,
) -> Result<(Entry, Entry), CommandError> {
    let primary = entry.build()?;
    if primary.account_id() == counter_account_id {
        return Err(CommandError::Validation(
            "a transfer needs two different accounts".to_owned(),
        ));
    }

    // Validate (read-only) before opening a write transaction: both accounts must
    // exist and share a currency (there is no conversion).
    let from = repo::account::get(pool, primary.account_id())
        .await?
        .ok_or(CommandError::NotFound)?;
    let to = repo::account::get(pool, counter_account_id)
        .await?
        .ok_or(CommandError::NotFound)?;
    if from.currency().code() != to.currency().code() {
        return Err(CommandError::Validation(
            "a transfer needs both accounts in the same currency".to_owned(),
        ));
    }

    // The counterpart side mirrors the entry: opposite kind, same amount, date,
    // note, and category (a transfer keeps one classification on both sides).
    // The id is a placeholder; the DB assigns the real one on insert.
    let counter = Entry::new(
        EntryId::new(0),
        counter_account_id,
        primary.amount(),
        primary.kind().opposite(),
        primary.date(),
        primary.note().map(str::to_owned),
        primary.category_id(),
    )?;

    let mut tx = pool.begin().await.map_err(RepoError::Sqlx)?;
    let saved_primary = repo::entry::insert(&mut *tx, &primary).await?;
    let saved_counter = repo::entry::insert(&mut *tx, &counter).await?;
    tx.commit().await.map_err(RepoError::Sqlx)?;
    Ok((saved_primary, saved_counter))
}

// ---- Recurring rule ---------------------------------------------------------

/// Creates a recurring rule.
///
/// # Errors
/// [`CommandError::Validation`] on invalid input; [`CommandError::Database`].
#[tauri::command]
pub async fn create_rule(
    state: State<'_, SqlitePool>,
    rule: NewRule,
) -> Result<RecurringRule, CommandError> {
    let draft = rule.build()?;
    Ok(repo::rule::insert(state.inner(), &draft).await?)
}

/// Lists all recurring rules for an account.
///
/// # Errors
/// [`CommandError::Database`] / [`CommandError::Corrupt`] on a database error.
#[tauri::command]
pub async fn list_rules(
    state: State<'_, SqlitePool>,
    account_id: AccountId,
) -> Result<Vec<RecurringRule>, CommandError> {
    Ok(repo::rule::for_account(state.inner(), account_id).await?)
}

/// Updates a recurring rule.
///
/// # Errors
/// [`CommandError::NotFound`] if it does not exist; [`CommandError::Database`].
#[tauri::command]
pub async fn update_rule(
    state: State<'_, SqlitePool>,
    rule: RecurringRule,
) -> Result<RecurringRule, CommandError> {
    require_found(repo::rule::update(state.inner(), &rule).await?)?;
    Ok(rule)
}

/// Deletes a recurring rule.
///
/// # Errors
/// [`CommandError::NotFound`] if it does not exist; [`CommandError::Database`].
#[tauri::command]
pub async fn delete_rule(
    state: State<'_, SqlitePool>,
    id: RecurringRuleId,
) -> Result<(), CommandError> {
    require_found(repo::rule::delete(state.inner(), id).await?)
}

// ---- Ledger queries ---------------------------------------------------------

pub(crate) async fn load_account_data(
    pool: &SqlitePool,
    account_id: AccountId,
) -> Result<(Account, Vec<Entry>, Vec<RecurringRule>), CommandError> {
    // One transaction so account, entries, and rules are read from a single
    // consistent snapshot even if another command writes concurrently.
    let mut tx = pool.begin().await.map_err(RepoError::Sqlx)?;
    let account = repo::account::get(&mut *tx, account_id)
        .await?
        .ok_or(CommandError::NotFound)?;
    let entries = repo::entry::for_account(&mut *tx, account_id).await?;
    let rules = repo::rule::for_account(&mut *tx, account_id).await?;
    let skips = repo::skip::for_account(&mut *tx, account_id).await?;
    tx.commit().await.map_err(RepoError::Sqlx)?;
    Ok((account, entries, attach_skips(rules, skips)))
}

/// Loads an account's recurring rules with their skips attached (verifying the
/// account exists), without reading entries — used by `month_occurrences`, which
/// only expands rules.
pub(crate) async fn load_account_rules(
    pool: &SqlitePool,
    account_id: AccountId,
) -> Result<Vec<RecurringRule>, CommandError> {
    let mut tx = pool.begin().await.map_err(RepoError::Sqlx)?;
    repo::account::get(&mut *tx, account_id)
        .await?
        .ok_or(CommandError::NotFound)?;
    let rules = repo::rule::for_account(&mut *tx, account_id).await?;
    let skips = repo::skip::for_account(&mut *tx, account_id).await?;
    tx.commit().await.map_err(RepoError::Sqlx)?;
    Ok(attach_skips(rules, skips))
}

/// Attaches each rule's skipped occurrence dates so its expansion omits them.
fn attach_skips(
    rules: Vec<RecurringRule>,
    skips: Vec<(RecurringRuleId, time::Date)>,
) -> Vec<RecurringRule> {
    use std::collections::HashMap;
    let mut by_rule: HashMap<RecurringRuleId, Vec<time::Date>> = HashMap::new();
    for (rule_id, date) in skips {
        by_rule.entry(rule_id).or_default().push(date);
    }
    rules
        .into_iter()
        .map(|rule| {
            let dates = by_rule.remove(&rule.id()).unwrap_or_default();
            rule.with_skips(dates)
        })
        .collect()
}

/// Number of months in `from..=to` (negative if `to` precedes `from`).
fn month_span(from: Month, to: Month) -> i64 {
    i64::from(to.year() - from.year()) * 12 + i64::from(to.month()) - i64::from(from.month())
}

/// Computes the budget summary for one month of an account.
///
/// # Errors
/// [`CommandError::NotFound`] if the account does not exist;
/// [`CommandError::Database`] / [`CommandError::Corrupt`] on a database error.
#[tauri::command]
pub async fn month_summary(
    state: State<'_, SqlitePool>,
    account_id: AccountId,
    month: Month,
) -> Result<MonthSummary, CommandError> {
    let (account, entries, rules) = load_account_data(state.inner(), account_id).await?;
    Ok(core_month_summary(
        month,
        account.opening_balance(),
        account.anchor(),
        &entries,
        &rules,
    ))
}

/// Computes contiguous budget summaries for `from..=to` of an account.
///
/// # Errors
/// [`CommandError::Validation`] if the range exceeds [`MAX_RANGE_MONTHS`];
/// [`CommandError::NotFound`] if the account does not exist;
/// [`CommandError::Database`] / [`CommandError::Corrupt`] on a database error.
#[tauri::command]
pub async fn summaries_for_range(
    state: State<'_, SqlitePool>,
    account_id: AccountId,
    from: Month,
    to: Month,
) -> Result<Vec<MonthSummary>, CommandError> {
    if month_span(from, to) > MAX_RANGE_MONTHS {
        return Err(CommandError::Validation(format!(
            "requested range exceeds {MAX_RANGE_MONTHS} months"
        )));
    }
    let (account, entries, rules) = load_account_data(state.inner(), account_id).await?;
    Ok(core_summaries_for_range(
        from,
        to,
        account.opening_balance(),
        account.anchor(),
        &entries,
        &rules,
    ))
}

/// Totals a month's expenses grouped by category for an account (descending by
/// amount; uncategorized expenses bucket under a `null` category id).
///
/// # Errors
/// [`CommandError::NotFound`] if the account does not exist;
/// [`CommandError::Database`] / [`CommandError::Corrupt`] on a database error.
#[tauri::command]
pub async fn expenses_by_category(
    state: State<'_, SqlitePool>,
    account_id: AccountId,
    month: Month,
) -> Result<Vec<CategoryExpense>, CommandError> {
    let (_account, entries, rules) = load_account_data(state.inner(), account_id).await?;
    Ok(core_expenses_by_category(month, &entries, &rules))
}

/// Expands an account's recurring rules into their occurrences within `month`
/// (read-only line items for the month view; they are not stored).
///
/// # Errors
/// [`CommandError::NotFound`] if the account does not exist;
/// [`CommandError::Database`] / [`CommandError::Corrupt`] on a database error.
#[tauri::command]
pub async fn month_occurrences(
    state: State<'_, SqlitePool>,
    account_id: AccountId,
    month: Month,
) -> Result<Vec<VirtualEntry>, CommandError> {
    let rules = load_account_rules(state.inner(), account_id).await?;
    Ok(rules
        .iter()
        .flat_map(|rule| rule.expand_in(month))
        .collect())
}

/// Removes a single occurrence of a recurring rule ("skip"), so the expansion
/// omits that date. The rule and its other occurrences are unaffected.
///
/// # Errors
/// [`CommandError::NotFound`] if the rule does not belong to `account_id`;
/// [`CommandError::Database`] on a database error.
#[tauri::command]
pub async fn skip_occurrence(
    state: State<'_, SqlitePool>,
    account_id: AccountId,
    occurrence: OccurrenceRef,
) -> Result<(), CommandError> {
    skip_occurrence_inner(state.inner(), account_id, occurrence).await
}

/// Confirms a rule belongs to `account_id` before a per-occurrence mutation,
/// so a crafted call can't touch another account's rule.
async fn require_rule_in_account(
    pool: &SqlitePool,
    rule_id: RecurringRuleId,
    account_id: AccountId,
) -> Result<(), CommandError> {
    if repo::rule::belongs_to(pool, rule_id, account_id).await? {
        Ok(())
    } else {
        Err(CommandError::NotFound)
    }
}

/// The `skip_occurrence` logic, separated from the command so it's testable.
pub(crate) async fn skip_occurrence_inner(
    pool: &SqlitePool,
    account_id: AccountId,
    occurrence: OccurrenceRef,
) -> Result<(), CommandError> {
    require_rule_in_account(pool, occurrence.rule_id, account_id).await?;
    Ok(repo::skip::add(pool, occurrence.rule_id, occurrence.date).await?)
}

/// "Detaches" a single occurrence into an editable standalone entry: records a
/// skip for the occurrence and inserts `entry` (its edited values) in one
/// transaction. The new entry is independent — later rule changes don't touch
/// it.
///
/// # Errors
/// [`CommandError::NotFound`] if the rule does not belong to `account_id`;
/// [`CommandError::Validation`] on invalid entry input; [`CommandError::Database`].
#[tauri::command]
pub async fn detach_occurrence(
    state: State<'_, SqlitePool>,
    account_id: AccountId,
    occurrence: OccurrenceRef,
    entry: NewEntry,
) -> Result<Entry, CommandError> {
    detach_occurrence_inner(state.inner(), account_id, occurrence, entry).await
}

/// The `detach_occurrence` logic, separated from the command so it's testable.
pub(crate) async fn detach_occurrence_inner(
    pool: &SqlitePool,
    account_id: AccountId,
    occurrence: OccurrenceRef,
    entry: NewEntry,
) -> Result<Entry, CommandError> {
    require_rule_in_account(pool, occurrence.rule_id, account_id).await?;
    let draft = entry.build()?;
    Ok(repo::skip::detach(pool, occurrence.rule_id, occurrence.date, &draft).await?)
}

// ---- Nextcloud backup / restore --------------------------------------------

/// Resolves the app-data directory (where the Nextcloud config lives).
fn app_data_dir(app: &AppHandle) -> Result<std::path::PathBuf, CommandError> {
    app.path().app_data_dir().map_err(|err| {
        log::error!("app data dir unavailable: {err}");
        CommandError::Backup("Couldn't access app storage.".into())
    })
}

/// Builds a `WebDAV` client from the stored config, or a friendly error if it
/// isn't configured yet.
fn client_for(config: &NextcloudConfig) -> Result<WebDav, CommandError> {
    if !config.is_configured() {
        return Err(CommandError::Backup(
            "Add your Nextcloud address, username, and app password first.".into(),
        ));
    }
    Ok(WebDav::new(
        &config.base_url,
        &config.username,
        &config.password,
    )?)
}

/// Returns the stored Nextcloud settings — never the password, only whether one
/// is set.
///
/// # Errors
/// [`CommandError::Backup`] if app storage is unavailable.
// Tauri's command macro requires `AppHandle` by value; it's an `Arc`-backed
// handle, so passing it by value is cheap and idiomatic.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn nextcloud_get_config(app: AppHandle) -> Result<NextcloudConfigView, CommandError> {
    let dir = app_data_dir(&app)?;
    Ok((&backup::load_config(&dir)).into())
}

/// Saves the Nextcloud address/username, and the app password when a non-empty
/// one is given (an empty password keeps the stored one).
///
/// # Errors
/// [`CommandError::Backup`] if the settings can't be written.
// Tauri's command macro requires owned `AppHandle`/`String` arguments.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn nextcloud_set_config(
    app: AppHandle,
    base_url: String,
    username: String,
    password: String,
) -> Result<(), CommandError> {
    let dir = app_data_dir(&app)?;
    backup::set_credentials(&dir, &base_url, &username, &password)
}

/// Verifies the stored Nextcloud address and credentials.
///
/// # Errors
/// [`CommandError::Backup`] if not configured, unreachable, or rejected.
#[tauri::command]
pub async fn nextcloud_test(app: AppHandle) -> Result<(), CommandError> {
    let dir = app_data_dir(&app)?;
    client_for(&backup::load_config(&dir))?.check().await?;
    Ok(())
}

/// Snapshots the database and uploads it to Nextcloud; returns the RFC-3339
/// timestamp of the backup.
///
/// # Errors
/// [`CommandError::Backup`] if not configured or the upload fails;
/// [`CommandError::Database`] on a snapshot error.
#[tauri::command]
pub async fn backup_now(
    app: AppHandle,
    state: State<'_, SqlitePool>,
) -> Result<String, CommandError> {
    let dir = app_data_dir(&app)?;
    let client = client_for(&backup::load_config(&dir))?;
    let bytes = backup::snapshot(state.inner(), &dir).await?;
    client.put_backup(bytes).await?;
    backup::mark_backed_up(&dir)
}

/// Downloads the Nextcloud backup and replaces all local data with it.
///
/// # Errors
/// [`CommandError::Backup`] if not configured, the download fails, or the file
/// is not a same-version Talea backup.
#[tauri::command]
pub async fn restore_now(app: AppHandle, state: State<'_, SqlitePool>) -> Result<(), CommandError> {
    let dir = app_data_dir(&app)?;
    let client = client_for(&backup::load_config(&dir))?;
    let bytes = client.get_backup().await?;
    backup::restore(state.inner(), &dir, &bytes).await
}
