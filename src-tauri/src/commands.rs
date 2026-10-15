//! Tauri command surface: the only bridge between the frontend and the domain.
//!
//! Commands are thin — they validate input, call the repository, and (for
//! summaries) the pure `talea_core` ledger functions. No domain logic lives
//! here. All take the shared [`SqlitePool`] from Tauri state and return a
//! [`CommandError`] on failure.

use sqlx::{Sqlite, SqlitePool, Transaction};
use tauri::{AppHandle, Manager, State};

use talea_core::{
    balance_as_of as core_balance_as_of, budget_status as core_budget_status,
    combine_summaries as core_combine_summaries, expenses_by_category as core_expenses_by_category,
    month_summary as core_month_summary, summaries_for_range as core_summaries_for_range, Account,
    AccountId, AccountKind, Budget, BudgetId, BudgetStatus, Category, CategoryExpense, CategoryId,
    Entry, EntryId, Money, Month, MonthSummary, RecurringRule, RecurringRuleId, VirtualEntry,
};

use crate::backup::{self, NextcloudConfig, NextcloudConfigView};
use crate::confirm;
use crate::db;
use crate::dto::{IsoDate, NewAccount, NewBudget, NewCategory, NewEntry, NewRule, OccurrenceRef};
use crate::error::{CommandError, RepoError};
use crate::repo;
use crate::repo::ledger::AccountData;
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

/// Validates a summary account's membership against the rest of the database:
/// loads the accounts its members name, then applies `core`'s cross-account
/// rules ([`Account::check_members`]). Normal accounts have no members to check.
/// It runs inside the caller's write transaction, so what it checks still
/// holds when the caller writes.
pub(crate) async fn validate_membership(
    tx: &mut Transaction<'_, Sqlite>,
    draft: &Account,
) -> Result<(), CommandError> {
    if draft.kind() == AccountKind::Normal {
        // No members to look up: `check_members` refuses any it has.
        return Ok(draft.check_members(|_| None)?);
    }
    let members = repo::account::get_many(&mut **tx, draft.members()).await?;
    Ok(draft.check_members(|id| members.iter().find(|member| member.id() == id))?)
}

/// Errors if `account_id` names a summary account — they hold no records, so
/// every write path rejects them (defense in depth behind the hidden UI).
pub(crate) async fn reject_if_summary(
    pool: &SqlitePool,
    account_id: AccountId,
) -> Result<(), CommandError> {
    let account = repo::account::get(pool, account_id)
        .await?
        .ok_or(CommandError::NotFound)?;
    if account.kind() == AccountKind::Summary {
        return Err(CommandError::Validation(
            "summary accounts are read-only and cannot hold entries or rules".to_owned(),
        ));
    }
    Ok(())
}

/// Creates an account.
///
/// # Errors
/// [`CommandError::Validation`] on invalid input (including summary membership);
/// [`CommandError::Database`] on a database error.
#[tauri::command]
pub async fn create_account(
    state: State<'_, SqlitePool>,
    account: NewAccount,
) -> Result<Account, CommandError> {
    create_account_inner(state.inner(), account).await
}

/// [`create_account`], checking a summary's members and inserting it in one
/// write transaction (see [`db::begin_write`]).
pub(crate) async fn create_account_inner(
    pool: &SqlitePool,
    account: NewAccount,
) -> Result<Account, CommandError> {
    let draft = account.build()?;
    let mut tx = db::begin_write(pool).await?;
    validate_membership(&mut tx, &draft).await?;
    let saved = repo::account::insert(&mut tx, &draft).await?;
    tx.commit().await.map_err(RepoError::from)?;
    Ok(saved)
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
/// An account's kind is fixed after creation. A normal account that belongs to a
/// summary cannot change its currency (it would break the summary's
/// same-currency invariant) — remove it from the summary first.
///
/// # Errors
/// [`CommandError::NotFound`] if it does not exist; [`CommandError::Validation`]
/// on invalid input or a forbidden change; [`CommandError::Database`].
#[tauri::command]
pub async fn update_account(
    state: State<'_, SqlitePool>,
    account: Account,
) -> Result<Account, CommandError> {
    update_account_inner(state.inner(), account).await
}

/// [`update_account`], checking and writing in one write transaction (see
/// [`db::begin_write`]).
pub(crate) async fn update_account_inner(
    pool: &SqlitePool,
    account: Account,
) -> Result<Account, CommandError> {
    let mut tx = db::begin_write(pool).await?;
    let existing = repo::account::get(&mut *tx, account.id())
        .await?
        .ok_or(CommandError::NotFound)?;
    if existing.kind() != account.kind() {
        return Err(CommandError::Validation(
            "an account's type cannot be changed after creation".to_owned(),
        ));
    }
    if account.kind() == AccountKind::Normal
        && existing.currency().code() != account.currency().code()
        && repo::account::is_member_of_any(&mut *tx, account.id()).await?
    {
        return Err(CommandError::Validation(
            "remove this account from its summary account before changing its currency".to_owned(),
        ));
    }
    validate_membership(&mut tx, &account).await?;
    // Read above in this same transaction, the row is still there; checked
    // anyway, so the repository needn't trust its caller.
    require_found(repo::account::update(&mut tx, &account).await?)?;
    tx.commit().await.map_err(RepoError::from)?;
    Ok(account)
}

/// Deletes an account (cascading its entries and rules).
///
/// Refuses to delete an account that still belongs to a summary account (it
/// would silently shrink — or empty — that summary). Remove it from the summary
/// first. Deleting a summary account itself is always fine.
///
/// # Errors
/// [`CommandError::NotFound`] if it does not exist; [`CommandError::Validation`]
/// if it is still a summary member; [`CommandError::Database`].
#[tauri::command]
pub async fn delete_account(
    state: State<'_, SqlitePool>,
    id: AccountId,
) -> Result<(), CommandError> {
    delete_account_inner(state.inner(), id).await
}

/// [`delete_account`], checking membership and deleting in one write
/// transaction (see [`db::begin_write`]): a summary saved in between can't take the
/// account as a member just before it goes.
pub(crate) async fn delete_account_inner(
    pool: &SqlitePool,
    id: AccountId,
) -> Result<(), CommandError> {
    let mut tx = db::begin_write(pool).await?;
    if repo::account::is_member_of_any(&mut *tx, id).await? {
        return Err(CommandError::Validation(
            "remove this account from its summary account before deleting it".to_owned(),
        ));
    }
    require_found(repo::account::delete(&mut tx, id).await?)?;
    tx.commit().await.map_err(RepoError::from)?;
    Ok(())
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
    reject_if_summary(state.inner(), draft.account_id()).await?;
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
    let pool = state.inner();
    let account = repo::account::get_full(pool, account_id)
        .await?
        .ok_or(CommandError::NotFound)?;
    if account.kind() == AccountKind::Summary {
        // Concatenate the members' entries; each entry carries its own
        // `account_id`, which the frontend uses to colour-tag rows by account.
        let mut all = Vec::new();
        for member_id in account.members() {
            all.extend(repo::entry::for_account(pool, *member_id).await?);
        }
        Ok(all)
    } else {
        Ok(repo::entry::for_account(pool, account_id).await?)
    }
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
    // Reject moving an entry onto a summary account (a crafted `account_id`).
    reject_if_summary(state.inner(), entry.account_id()).await?;
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

    // Both accounts must exist and share a currency (there is no conversion),
    // checked in the transaction that writes, under its lock (see
    // `db::begin_write`): an account's currency changed in between would
    // otherwise mirror the amount into the wrong currency.
    let mut tx = db::begin_write(pool).await?;
    let from = repo::account::get(&mut *tx, primary.account_id())
        .await?
        .ok_or(CommandError::NotFound)?;
    let to = repo::account::get(&mut *tx, counter_account_id)
        .await?
        .ok_or(CommandError::NotFound)?;
    if from.kind() == AccountKind::Summary || to.kind() == AccountKind::Summary {
        return Err(CommandError::Validation(
            "summary accounts are read-only and cannot take transfers".to_owned(),
        ));
    }
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
    reject_if_summary(state.inner(), draft.account_id()).await?;
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
    reject_if_summary(state.inner(), rule.account_id()).await?;
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

// ---- Budget -----------------------------------------------------------------

/// Cross-entity rules `core` can't see: the budget's account must exist and not
/// be a summary (summaries hold no records of their own), its category must
/// exist, and the account must not already budget that category.
///
/// `excluding` is the budget being updated, so an unchanged row doesn't collide
/// with itself. The `UNIQUE (account_id, category_id)` constraint remains the
/// race-proof backstop behind this friendlier check.
async fn validate_budget(
    pool: &SqlitePool,
    budget: &Budget,
    excluding: Option<BudgetId>,
) -> Result<(), CommandError> {
    reject_if_summary(pool, budget.account_id()).await?;
    if !repo::category::exists(pool, budget.category_id()).await? {
        return Err(CommandError::Validation(
            "that category no longer exists".to_owned(),
        ));
    }
    if repo::budget::exists_for_category(pool, budget.account_id(), budget.category_id(), excluding)
        .await?
    {
        return Err(CommandError::Validation(
            "this category already has a budget on this account".to_owned(),
        ));
    }
    Ok(())
}

/// Creates a category budget.
///
/// # Errors
/// [`CommandError::Validation`] on invalid input, a summary account, a missing
/// category, or a category that is already budgeted;
/// [`CommandError::Database`] on a database error.
#[tauri::command]
pub async fn create_budget(
    state: State<'_, SqlitePool>,
    budget: NewBudget,
) -> Result<Budget, CommandError> {
    create_budget_inner(state.inner(), budget).await
}

/// `create_budget` against a pool, separated so it's testable.
pub(crate) async fn create_budget_inner(
    pool: &SqlitePool,
    budget: NewBudget,
) -> Result<Budget, CommandError> {
    let draft = budget.build()?;
    validate_budget(pool, &draft, None).await?;
    Ok(repo::budget::insert(pool, &draft).await?)
}

/// Lists an account's budgets. A summary account concatenates its members'.
///
/// # Errors
/// [`CommandError::NotFound`] if the account does not exist;
/// [`CommandError::Database`] / [`CommandError::Corrupt`] on a database error.
#[tauri::command]
pub async fn list_budgets(
    state: State<'_, SqlitePool>,
    account_id: AccountId,
) -> Result<Vec<Budget>, CommandError> {
    let pool = state.inner();
    let mut out = Vec::new();
    for id in ledger_source_accounts(pool, account_id).await? {
        out.extend(repo::budget::for_account(pool, id).await?);
    }
    Ok(out)
}

/// Updates a budget.
///
/// # Errors
/// [`CommandError::Validation`] as for [`create_budget`];
/// [`CommandError::NotFound`] if it does not exist; [`CommandError::Database`].
#[tauri::command]
pub async fn update_budget(
    state: State<'_, SqlitePool>,
    budget: Budget,
) -> Result<Budget, CommandError> {
    update_budget_inner(state.inner(), budget).await
}

/// `update_budget` against a pool, separated so it's testable.
pub(crate) async fn update_budget_inner(
    pool: &SqlitePool,
    budget: Budget,
) -> Result<Budget, CommandError> {
    // Checked against the *payload's* account_id, so a crafted call can't move a
    // budget onto a summary account (as `update_entry` does for entries).
    validate_budget(pool, &budget, Some(budget.id())).await?;
    require_found(repo::budget::update(pool, &budget).await?)?;
    Ok(budget)
}

/// Deletes a budget.
///
/// # Errors
/// [`CommandError::NotFound`] if it does not exist; [`CommandError::Database`].
#[tauri::command]
pub async fn delete_budget(state: State<'_, SqlitePool>, id: BudgetId) -> Result<(), CommandError> {
    // No summary check: a summary account owns no budgets, so there is nothing
    // to guard here.
    require_found(repo::budget::delete(state.inner(), id).await?)
}

/// Each budget's progress within `month` for an account.
///
/// # Errors
/// [`CommandError::NotFound`] if the account does not exist;
/// [`CommandError::Database`] / [`CommandError::Corrupt`] on a database error.
#[tauri::command]
pub async fn month_budgets(
    state: State<'_, SqlitePool>,
    account_id: AccountId,
    month: Month,
) -> Result<Vec<BudgetStatus>, CommandError> {
    month_budgets_inner(state.inner(), account_id, month).await
}

/// `month_budgets` against a pool (summary-aware), separated so it's testable.
///
/// A summary account **concatenates** its members' rows rather than merging them
/// by category. That keeps `Σ status.reserved` equal to the month summary's
/// `reserved`, which is itself a per-member sum: one member overspending a
/// category must not cancel another member's reservation in it.
pub(crate) async fn month_budgets_inner(
    pool: &SqlitePool,
    account_id: AccountId,
    month: Month,
) -> Result<Vec<BudgetStatus>, CommandError> {
    let mut out = Vec::new();
    for id in ledger_source_accounts(pool, account_id).await? {
        let data = load_account_data(pool, id).await?;
        // Months before the account's anchor predate its ledger: `month_summary`
        // declares them inert (zero activity, zero `reserved`), so budgets must be
        // inert here too — otherwise a row would claim a reservation the headline
        // figure doesn't have, breaking `Σ status.reserved == summary.reserved`.
        if month < data.account.anchor() {
            continue;
        }
        out.extend(core_budget_status(
            month,
            &data.budgets,
            &data.entries,
            &data.rules,
        ));
    }
    Ok(out)
}

// ---- Ledger queries ---------------------------------------------------------

/// The accounts whose own records back `account_id`'s figures: a summary
/// account's members, or the account itself.
///
/// # Errors
/// [`CommandError::NotFound`] if the account does not exist.
async fn ledger_source_accounts(
    pool: &SqlitePool,
    account_id: AccountId,
) -> Result<Vec<AccountId>, CommandError> {
    let account = repo::account::get_full(pool, account_id)
        .await?
        .ok_or(CommandError::NotFound)?;
    Ok(if account.kind() == AccountKind::Summary {
        account.members().to_vec()
    } else {
        vec![account_id]
    })
}

pub(crate) async fn load_account_data(
    pool: &SqlitePool,
    account_id: AccountId,
) -> Result<AccountData, CommandError> {
    // One transaction so account, entries, rules, and budgets are read from a
    // single consistent snapshot even if another command writes concurrently.
    // Budgets belong here rather than in a sibling loader: `reserved` is derived
    // from budgets *and* entries together, so a second read transaction would let
    // a concurrent write land between the two halves of one figure.
    let mut tx = pool.begin().await.map_err(RepoError::Sqlx)?;
    let data = repo::ledger::read(&mut tx, account_id)
        .await?
        .ok_or(CommandError::NotFound)?;
    tx.commit().await.map_err(RepoError::Sqlx)?;
    Ok(data)
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
    let rules = repo::ledger::rules(&mut tx, account_id).await?;
    tx.commit().await.map_err(RepoError::Sqlx)?;
    Ok(rules)
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
    month_summary_inner(state.inner(), account_id, month).await
}

/// `month_summary` against a pool (summary-aware), separated so it's testable.
pub(crate) async fn month_summary_inner(
    pool: &SqlitePool,
    account_id: AccountId,
    month: Month,
) -> Result<MonthSummary, CommandError> {
    let account = repo::account::get_full(pool, account_id)
        .await?
        .ok_or(CommandError::NotFound)?;
    if account.kind() == AccountKind::Summary {
        // Combine each member's own summary (members share the currency). Each
        // member's budgets run against that member's own entries, so one member
        // overspending a category can't cancel another's reservation in it.
        let mut parts = Vec::with_capacity(account.members().len());
        for member_id in account.members() {
            let data = load_account_data(pool, *member_id).await?;
            parts.push(core_month_summary(
                month,
                data.account.opening_balance(),
                data.account.anchor(),
                &data.entries,
                &data.rules,
                &data.budgets,
            ));
        }
        Ok(core_combine_summaries(month, &parts))
    } else {
        let data = load_account_data(pool, account_id).await?;
        Ok(core_month_summary(
            month,
            data.account.opening_balance(),
            data.account.anchor(),
            &data.entries,
            &data.rules,
            &data.budgets,
        ))
    }
}

/// Computes an account's running balance at the end of `date`. For today, that
/// is what the real account should show if every transaction was recorded
/// (`docs/DESIGN.md` §13).
///
/// The frontend supplies the date from the device's local clock: `core` has no
/// clock by design, and `time`'s local-offset lookup is unreliable once the
/// process is multithreaded.
///
/// # Errors
/// [`CommandError::NotFound`] if the account does not exist;
/// [`CommandError::Database`] / [`CommandError::Corrupt`] on a database error.
#[tauri::command]
pub async fn balance_as_of(
    state: State<'_, SqlitePool>,
    account_id: AccountId,
    date: IsoDate,
) -> Result<Money, CommandError> {
    balance_as_of_inner(state.inner(), account_id, date.get()).await
}

/// `balance_as_of` against a pool (summary-aware), separated so it's testable.
pub(crate) async fn balance_as_of_inner(
    pool: &SqlitePool,
    account_id: AccountId,
    as_of: time::Date,
) -> Result<Money, CommandError> {
    // A summary account keeps no ledger of its own: its balance is its members'
    // total (they share the currency), each against its own opening balance and
    // anchor — the per-member sum `combine_summaries` applies to month figures.
    let mut total = Money::zero();
    for id in ledger_source_accounts(pool, account_id).await? {
        // Each ledger's opening balance and anchor come from the same snapshot
        // as its entries, so no one ledger's figure mixes two reads.
        let data = load_account_data(pool, id).await?;
        let balance = core_balance_as_of(
            as_of,
            data.account.opening_balance(),
            data.account.anchor(),
            &data.entries,
            &data.rules,
        );
        // With every input capped this can't overflow (DESIGN.md §9), but it is a
        // new sum across ledgers, so check it rather than risk an abort. Only
        // out-of-range data could trip it, which is what `Corrupt` reports.
        total = total.checked_add(balance).ok_or(CommandError::Corrupt)?;
    }
    Ok(total)
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
    let pool = state.inner();
    let account = repo::account::get_full(pool, account_id)
        .await?
        .ok_or(CommandError::NotFound)?;
    if account.kind() == AccountKind::Summary {
        // Each member's range covers the same months (`from..=to`); combine them
        // month-by-month. The empty-data baseline gives the month skeleton so the
        // result is correct even for a summary with no members.
        let months: Vec<Month> =
            core_summaries_for_range(from, to, talea_core::Money::zero(), from, &[], &[], &[])
                .into_iter()
                .map(|s| s.month)
                .collect();
        let mut member_ranges = Vec::with_capacity(account.members().len());
        for member_id in account.members() {
            let data = load_account_data(pool, *member_id).await?;
            member_ranges.push(core_summaries_for_range(
                from,
                to,
                data.account.opening_balance(),
                data.account.anchor(),
                &data.entries,
                &data.rules,
                &data.budgets,
            ));
        }
        Ok(months
            .into_iter()
            .enumerate()
            .map(|(i, month)| {
                let parts: Vec<MonthSummary> = member_ranges
                    .iter()
                    .filter_map(|range| range.get(i).cloned())
                    .collect();
                core_combine_summaries(month, &parts)
            })
            .collect())
    } else {
        let data = load_account_data(pool, account_id).await?;
        Ok(core_summaries_for_range(
            from,
            to,
            data.account.opening_balance(),
            data.account.anchor(),
            &data.entries,
            &data.rules,
            &data.budgets,
        ))
    }
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
    expenses_by_category_inner(state.inner(), account_id, month).await
}

/// `expenses_by_category` against a pool (summary-aware), separated so it's
/// testable.
pub(crate) async fn expenses_by_category_inner(
    pool: &SqlitePool,
    account_id: AccountId,
    month: Month,
) -> Result<Vec<CategoryExpense>, CommandError> {
    // Pool every relevant account's entries and rules, then run the existing
    // per-account categorisation once over the combined set (members share the
    // currency, so the totals are comparable).
    let mut entries = Vec::new();
    let mut rules = Vec::new();
    for id in ledger_source_accounts(pool, account_id).await? {
        let data = load_account_data(pool, id).await?;
        entries.extend(data.entries);
        rules.extend(data.rules);
    }
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
    let pool = state.inner();
    let mut out = Vec::new();
    for id in ledger_source_accounts(pool, account_id).await? {
        let rules = load_account_rules(pool, id).await?;
        out.extend(rules.iter().flat_map(|rule| rule.expand_in(month)));
    }
    Ok(out)
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
    reject_if_summary(pool, account_id).await?;
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
    reject_if_summary(pool, account_id).await?;
    require_rule_in_account(pool, occurrence.rule_id, account_id).await?;
    let draft = entry.build()?;
    // The detached entry carries its own `account_id`; never let it land on a
    // summary account (a crafted payload could differ from `account_id`).
    reject_if_summary(pool, draft.account_id()).await?;
    Ok(repo::skip::detach(pool, occurrence.rule_id, occurrence.date, &draft).await?)
}

// ---- Nextcloud backup / restore --------------------------------------------

/// Runs backups and restores one at a time, each from its first step to its
/// last, network included. Side by side, a backup's upload could land after a
/// restore's download, leaving this device with the server's old copy and the
/// server with this device's: an outcome neither order would give. Taken
/// before `backup`'s own scratch-file lock, and not re-entrant: nothing called
/// under it may take it again.
static TRANSFER: tauri::async_runtime::Mutex<()> = tauri::async_runtime::Mutex::const_new(());

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
/// Async, as every command here is, so it runs off the main thread, which
/// also draws the native confirmation (see `confirm.rs`).
///
/// # Errors
/// [`CommandError::Backup`] if app storage is unavailable.
#[tauri::command]
pub async fn nextcloud_get_config(app: AppHandle) -> Result<NextcloudConfigView, CommandError> {
    let dir = app_data_dir(&app)?;
    Ok((&backup::load_config(&dir)).into())
}

/// Saves the Nextcloud address/username, and the app password when a non-empty
/// one is given; returns whether it saved them. A new server is saved only
/// once the user confirms it in a native dialog (see [`confirm`]), so
/// declining is an answer, not an error. An empty password keeps the stored
/// one for the same address and username only; for any other it drops it, so a
/// stored password never follows a changed address (see
/// [`backup::save_settings`]).
///
/// # Errors
/// [`CommandError::Backup`] if the password is longer than any app password,
/// a new address or username isn't usable, the dialog can't be asked (another
/// is open, or the questions are paused after repeated declines), its confirm
/// was pressed too soon to have been read, the settings changed while it was
/// open, or they can't be written.
#[tauri::command]
pub async fn nextcloud_set_config(
    app: AppHandle,
    base_url: String,
    username: String,
    password: String,
) -> Result<bool, CommandError> {
    let dir = app_data_dir(&app)?;
    backup::save_settings(&dir, &base_url, &username, &password, |server| {
        confirm::confirm_server(&app, server)
    })
    .await
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
/// timestamp of the backup, or `None` if the settings moved to another server
/// while it uploaded, since the backup isn't on that one.
///
/// # Errors
/// [`CommandError::Backup`] if not configured or the upload fails;
/// [`CommandError::Database`] on a snapshot error.
#[tauri::command]
pub async fn backup_now(
    app: AppHandle,
    state: State<'_, SqlitePool>,
) -> Result<Option<String>, CommandError> {
    let _transfer = TRANSFER.lock().await;
    let dir = app_data_dir(&app)?;
    let (pool, scratch) = (state.inner(), dir.as_path());
    backup::back_up(&dir, |config| async move {
        let client = client_for(&config)?;
        let bytes = backup::snapshot(pool, scratch).await?;
        client.put_backup(bytes).await?;
        Ok(())
    })
    .await
}

/// Downloads the Nextcloud backup and replaces all local data with it.
///
/// # Errors
/// [`CommandError::Backup`] if not configured, the download fails, or the file
/// is not a same-version Talea backup.
#[tauri::command]
pub async fn restore_now(app: AppHandle, state: State<'_, SqlitePool>) -> Result<(), CommandError> {
    let _transfer = TRANSFER.lock().await;
    let dir = app_data_dir(&app)?;
    let client = client_for(&backup::load_config(&dir))?;
    let bytes = client.get_backup().await?;
    backup::restore(state.inner(), &dir, &bytes).await
}
