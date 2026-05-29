//! Recurring-rule persistence.

use sqlx::SqlitePool;
use talea_core::{AccountId, CategoryId, Frequency, Money, RecurringRule, RecurringRuleId};

use crate::error::RepoError;
use crate::repo::map::{
    date_from_text, date_to_text, freq_unit_from_text, freq_unit_to_text, id_from_rowid, id_to_i64,
    kind_from_text, kind_to_text, rule_end_from_columns, rule_end_to_columns,
};

struct RuleRow {
    id: i64,
    account_id: i64,
    amount: String,
    kind: String,
    note: Option<String>,
    category_id: Option<i64>,
    start_date: String,
    end_kind: String,
    end_date: Option<String>,
    freq_unit: String,
    freq_interval: i64,
}

fn row_to_rule(row: RuleRow) -> Result<RecurringRule, RepoError> {
    let amount = Money::try_from_str(&row.amount)
        .map_err(|e| RepoError::Corrupt(format!("invalid money {:?}: {e}", row.amount)))?;
    let kind = kind_from_text(&row.kind)?;
    let category = row.category_id.map(|c| CategoryId::new(id_from_rowid(c)));
    let start_date = date_from_text(&row.start_date)?;
    let end = rule_end_from_columns(&row.end_kind, row.end_date)?;
    let interval = u32::try_from(row.freq_interval).map_err(|_| {
        RepoError::Corrupt(format!("freq_interval {} out of range", row.freq_interval))
    })?;
    let frequency = Frequency::new(freq_unit_from_text(&row.freq_unit)?, interval)
        .map_err(|e| RepoError::corrupt(&e))?;

    RecurringRule::new(
        RecurringRuleId::new(id_from_rowid(row.id)),
        AccountId::new(id_from_rowid(row.account_id)),
        amount,
        kind,
        row.note,
        category,
        start_date,
        end,
        frequency,
    )
    .map_err(|e| RepoError::corrupt(&e))
}

/// Inserts a validated draft rule and returns it with the assigned id.
///
/// # Errors
/// [`RepoError`] on a database error (e.g. a non-existent `account_id`).
pub async fn insert(pool: &SqlitePool, draft: &RecurringRule) -> Result<RecurringRule, RepoError> {
    let account_id = id_to_i64(draft.account_id().get())?;
    let amount = draft.amount().to_string();
    let kind = kind_to_text(draft.kind());
    let note = draft.note();
    let category_id = draft
        .category_id()
        .map(|c| id_to_i64(c.get()))
        .transpose()?;
    let start_date = date_to_text(draft.start_date());
    let (end_kind, end_date) = rule_end_to_columns(draft.end());
    let freq_unit = freq_unit_to_text(draft.frequency().unit());
    let freq_interval = i64::from(draft.frequency().interval());

    let rec = sqlx::query!(
        r#"INSERT INTO recurring_rule
               (account_id, amount, kind, note, category_id, start_date,
                end_kind, end_date, freq_unit, freq_interval)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           RETURNING id AS "id!""#,
        account_id,
        amount,
        kind,
        note,
        category_id,
        start_date,
        end_kind,
        end_date,
        freq_unit,
        freq_interval
    )
    .fetch_one(pool)
    .await?;

    row_to_rule(RuleRow {
        id: rec.id,
        account_id,
        amount,
        kind: kind.to_owned(),
        note: draft.note().map(str::to_owned),
        category_id,
        start_date,
        end_kind: end_kind.to_owned(),
        end_date,
        freq_unit: freq_unit.to_owned(),
        freq_interval,
    })
}

/// All recurring rules for an account, ordered by id.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn for_account<'e, E>(
    executor: E,
    account_id: AccountId,
) -> Result<Vec<RecurringRule>, RepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let key = id_to_i64(account_id.get())?;
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", account_id AS "account_id!", amount AS "amount!",
                  kind AS "kind!", note, category_id, start_date AS "start_date!",
                  end_kind AS "end_kind!", end_date, freq_unit AS "freq_unit!",
                  freq_interval AS "freq_interval!"
           FROM recurring_rule WHERE account_id = ? ORDER BY id"#,
        key
    )
    .fetch_all(executor)
    .await?;

    rows.into_iter()
        .map(|r| {
            row_to_rule(RuleRow {
                id: r.id,
                account_id: r.account_id,
                amount: r.amount,
                kind: r.kind,
                note: r.note,
                category_id: r.category_id,
                start_date: r.start_date,
                end_kind: r.end_kind,
                end_date: r.end_date,
                freq_unit: r.freq_unit,
                freq_interval: r.freq_interval,
            })
        })
        .collect()
}

/// Updates a recurring rule in place. Returns `true` if a row was updated.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn update(pool: &SqlitePool, rule: &RecurringRule) -> Result<bool, RepoError> {
    let id = id_to_i64(rule.id().get())?;
    let account_id = id_to_i64(rule.account_id().get())?;
    let amount = rule.amount().to_string();
    let kind = kind_to_text(rule.kind());
    let note = rule.note();
    let category_id = rule.category_id().map(|c| id_to_i64(c.get())).transpose()?;
    let start_date = date_to_text(rule.start_date());
    let (end_kind, end_date) = rule_end_to_columns(rule.end());
    let freq_unit = freq_unit_to_text(rule.frequency().unit());
    let freq_interval = i64::from(rule.frequency().interval());

    let result = sqlx::query!(
        r#"UPDATE recurring_rule
           SET account_id = ?, amount = ?, kind = ?, note = ?, category_id = ?,
               start_date = ?, end_kind = ?, end_date = ?, freq_unit = ?, freq_interval = ?
           WHERE id = ?"#,
        account_id,
        amount,
        kind,
        note,
        category_id,
        start_date,
        end_kind,
        end_date,
        freq_unit,
        freq_interval,
        id
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Deletes a recurring rule. Returns `true` if a row was deleted.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn delete(pool: &SqlitePool, id: RecurringRuleId) -> Result<bool, RepoError> {
    let key = id_to_i64(id.get())?;
    let result = sqlx::query!("DELETE FROM recurring_rule WHERE id = ?", key)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
