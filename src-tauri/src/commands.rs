//! Tauri command surface: the only bridge between the frontend and the domain.
//!
//! Commands are thin — they validate input, call the repository, and (for
//! summaries) the pure `talea_core` ledger functions. No domain logic lives
//! here. All take the shared [`SqlitePool`] from Tauri state and return a
//! [`CommandError`] on failure.

use sqlx::SqlitePool;
use tauri::State;

use talea_core::{
    month_summary as core_month_summary, summaries_for_range as core_summaries_for_range, Account,
    AccountId, Category, CategoryId, Entry, EntryId, Month, MonthSummary, RecurringRule,
    RecurringRuleId,
};

use crate::dto::{NewAccount, NewCategory, NewEntry, NewRule};
use crate::error::{CommandError, RepoError};
use crate::repo;

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
    tx.commit().await.map_err(RepoError::Sqlx)?;
    Ok((account, entries, rules))
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
