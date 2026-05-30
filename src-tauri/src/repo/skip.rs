//! Per-occurrence "skip" persistence for recurring rules.
//!
//! A skip records one occurrence date the user removed from a rule; the
//! expansion omits it (see [`RecurringRule::with_skips`](talea_core::RecurringRule::with_skips)).
//! "Editing" an occurrence is a skip plus a normal standalone entry carrying the
//! edited values, written together so the two never diverge.

use sqlx::SqlitePool;
use talea_core::{AccountId, Entry, RecurringRuleId};
use time::Date;

use crate::error::RepoError;
use crate::repo::map::{date_from_text, date_to_text, id_from_rowid, id_to_i64};

/// Records a skipped occurrence date for a rule (idempotent).
///
/// # Errors
/// [`RepoError`] on a database error (e.g. a non-existent `rule_id`).
pub async fn add(pool: &SqlitePool, rule_id: RecurringRuleId, date: Date) -> Result<(), RepoError> {
    let id = id_to_i64(rule_id.get())?;
    let date = date_to_text(date);
    sqlx::query!(
        "INSERT OR IGNORE INTO rule_skip (rule_id, occurrence_date) VALUES (?, ?)",
        id,
        date
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// All `(rule_id, occurrence_date)` skips for an account's rules.
///
/// # Errors
/// [`RepoError`] on a database error or an unreadable date.
pub async fn for_account<'e, E>(
    executor: E,
    account_id: AccountId,
) -> Result<Vec<(RecurringRuleId, Date)>, RepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let key = id_to_i64(account_id.get())?;
    let rows = sqlx::query!(
        r#"SELECT s.rule_id AS "rule_id!", s.occurrence_date AS "occurrence_date!"
           FROM rule_skip s
           JOIN recurring_rule r ON r.id = s.rule_id
           WHERE r.account_id = ?"#,
        key
    )
    .fetch_all(executor)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok((
                RecurringRuleId::new(id_from_rowid(r.rule_id)),
                date_from_text(&r.occurrence_date)?,
            ))
        })
        .collect()
}

/// Atomically "detaches" an occurrence: records the skip and inserts the
/// standalone replacement entry in one transaction, returning the new entry.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn detach(
    pool: &SqlitePool,
    rule_id: RecurringRuleId,
    date: Date,
    draft: &Entry,
) -> Result<Entry, RepoError> {
    let id = id_to_i64(rule_id.get())?;
    let date_text = date_to_text(date);

    let mut tx = pool.begin().await?;
    // Plain INSERT (not OR IGNORE): if this occurrence was already skipped, the
    // PK conflict aborts the transaction so we never create a second standalone
    // entry for one occurrence. (The UI can't reach this — a skipped occurrence
    // disappears from the list — but the guard keeps a crafted call honest.)
    sqlx::query!(
        "INSERT INTO rule_skip (rule_id, occurrence_date) VALUES (?, ?)",
        id,
        date_text
    )
    .execute(&mut *tx)
    .await?;
    let entry = crate::repo::entry::insert(&mut *tx, draft).await?;
    tx.commit().await?;
    Ok(entry)
}
