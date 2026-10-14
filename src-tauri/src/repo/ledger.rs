//! One account's ledger inputs — the account, its entries, its recurring rules
//! with their skips attached, and its budgets — read together. This is the one
//! read path for them, shared by the commands and by restore, which validates a
//! downloaded backup with it before committing, so the two can never disagree
//! about what the app can read.

use std::collections::HashMap;

use talea_core::{Account, AccountId, Budget, Entry, RecurringRule, RecurringRuleId};

use crate::error::RepoError;

/// One account's ledger inputs. Read through [`read`], `account` is as
/// `account::get` returns it, so a summary comes without its members.
pub struct AccountData {
    pub account: Account,
    pub entries: Vec<Entry>,
    pub rules: Vec<RecurringRule>,
    pub budgets: Vec<Budget>,
}

/// Reads one account's ledger inputs on `conn`, or `None` if there is no such
/// account. The caller chooses the snapshot: the commands read inside a
/// transaction of their own, restore inside the one it hasn't committed yet.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn read(
    conn: &mut sqlx::SqliteConnection,
    account_id: AccountId,
) -> Result<Option<AccountData>, RepoError> {
    let Some(account) = super::account::get(&mut *conn, account_id).await? else {
        return Ok(None);
    };
    records(conn, account).await.map(Some)
}

/// Reads the ledger inputs that belong to `account`, which the caller has
/// already loaded: its entries, its rules and its budgets.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn records(
    conn: &mut sqlx::SqliteConnection,
    account: Account,
) -> Result<AccountData, RepoError> {
    let account_id = account.id();
    let entries = super::entry::for_account(&mut *conn, account_id).await?;
    let rules = rules(&mut *conn, account_id).await?;
    let budgets = super::budget::for_account(&mut *conn, account_id).await?;
    Ok(AccountData {
        account,
        entries,
        rules,
        budgets,
    })
}

/// Reads an account's recurring rules with their skipped dates attached, the
/// only way rules are read for a ledger.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn rules(
    conn: &mut sqlx::SqliteConnection,
    account_id: AccountId,
) -> Result<Vec<RecurringRule>, RepoError> {
    let rules = super::rule::for_account(&mut *conn, account_id).await?;
    let skips = super::skip::for_account(&mut *conn, account_id).await?;
    Ok(attach_skips(rules, skips))
}

/// Attaches each rule's skipped occurrence dates so its expansion omits them.
fn attach_skips(
    rules: Vec<RecurringRule>,
    skips: Vec<(RecurringRuleId, time::Date)>,
) -> Vec<RecurringRule> {
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
