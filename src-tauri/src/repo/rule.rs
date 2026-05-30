//! Recurring-rule persistence.
//!
//! A rule's base amount lives on `recurring_rule.amount` (effective from its
//! `start_date`); any later amount breakpoints live in the `rule_amount` child
//! table. The repository assembles the two into the domain's
//! [`AmountSegment`](talea_core::AmountSegment) history and splits them back out
//! on write.

use sqlx::SqlitePool;
use talea_core::{
    AccountId, AmountSegment, CategoryId, Frequency, Money, RecurringRule, RecurringRuleId,
};

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

fn money_from_text(text: &str) -> Result<Money, RepoError> {
    Money::try_from_str(text)
        .map_err(|e| RepoError::Corrupt(format!("invalid money {text:?}: {e}")))
}

/// Builds a rule from its base row plus its extra amount breakpoints (each a
/// `(effective_from, amount)` text pair, expected sorted ascending by date).
fn assemble_rule(
    row: RuleRow,
    extra_segments: Vec<(String, String)>,
) -> Result<RecurringRule, RepoError> {
    let kind = kind_from_text(&row.kind)?;
    let category = row.category_id.map(|c| CategoryId::new(id_from_rowid(c)));
    let start_date = date_from_text(&row.start_date)?;
    let end = rule_end_from_columns(&row.end_kind, row.end_date)?;
    let interval = u32::try_from(row.freq_interval).map_err(|_| {
        RepoError::Corrupt(format!("freq_interval {} out of range", row.freq_interval))
    })?;
    let frequency = Frequency::new(freq_unit_from_text(&row.freq_unit)?, interval)
        .map_err(|e| RepoError::corrupt(&e))?;

    // Base segment (at start_date) followed by the stored breakpoints.
    let mut amounts = vec![AmountSegment::new(
        start_date,
        money_from_text(&row.amount)?,
    )];
    for (from, amount) in extra_segments {
        amounts.push(AmountSegment::new(
            date_from_text(&from)?,
            money_from_text(&amount)?,
        ));
    }

    RecurringRule::new_with_amounts(
        RecurringRuleId::new(id_from_rowid(row.id)),
        AccountId::new(id_from_rowid(row.account_id)),
        amounts,
        kind,
        row.note,
        category,
        start_date,
        end,
        frequency,
    )
    .map_err(|e| RepoError::corrupt(&e))
}

/// The rule's amount breakpoints beyond the base (the segments after the first),
/// as `(effective_from, amount)` strings ready for `rule_amount`.
fn extra_segments(rule: &RecurringRule) -> Vec<(String, String)> {
    rule.amounts()
        .iter()
        .skip(1)
        .map(|seg| (date_to_text(seg.effective_from()), seg.amount().to_string()))
        .collect()
}

/// Inserts a validated draft rule (and any amount breakpoints) in one
/// transaction, returning it with the assigned id.
///
/// # Errors
/// [`RepoError`] on a database error (e.g. a non-existent `account_id`).
pub async fn insert(pool: &SqlitePool, draft: &RecurringRule) -> Result<RecurringRule, RepoError> {
    let account_id = id_to_i64(draft.account_id().get())?;
    let amount = draft.base_amount().to_string();
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
    let segments = extra_segments(draft);

    let mut tx = pool.begin().await?;
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
    .fetch_one(&mut *tx)
    .await?;

    for (effective_from, seg_amount) in &segments {
        sqlx::query!(
            "INSERT INTO rule_amount (rule_id, effective_from, amount) VALUES (?, ?, ?)",
            rec.id,
            effective_from,
            seg_amount
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    assemble_rule(
        RuleRow {
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
        },
        segments,
    )
}

/// All recurring rules for an account (with their amount history), ordered by id.
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
    // One LEFT JOIN so each rule arrives with its breakpoints from a single
    // consistent read; rows are ordered (rule id, then breakpoint date) so the
    // grouping below stays a simple sequential scan.
    let rows = sqlx::query!(
        r#"SELECT r.id AS "id!", r.account_id AS "account_id!", r.amount AS "amount!",
                  r.kind AS "kind!", r.note, r.category_id, r.start_date AS "start_date!",
                  r.end_kind AS "end_kind!", r.end_date, r.freq_unit AS "freq_unit!",
                  r.freq_interval AS "freq_interval!",
                  ra.effective_from AS "seg_from?", ra.amount AS "seg_amount?"
           FROM recurring_rule r
           LEFT JOIN rule_amount ra ON ra.rule_id = r.id
           WHERE r.account_id = ?
           ORDER BY r.id, ra.effective_from"#,
        key
    )
    .fetch_all(executor)
    .await?;

    let mut rules = Vec::new();
    let mut current: Option<(RuleRow, Vec<(String, String)>)> = None;
    for r in rows {
        // New rule id → finish the previous group.
        if current.as_ref().map_or(true, |(row, _)| row.id != r.id) {
            if let Some((row, segments)) = current.take() {
                rules.push(assemble_rule(row, segments)?);
            }
            current = Some((
                RuleRow {
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
                },
                Vec::new(),
            ));
        }
        if let (Some(from), Some(amount)) = (r.seg_from, r.seg_amount) {
            if let Some((_, segments)) = current.as_mut() {
                segments.push((from, amount));
            }
        }
    }
    if let Some((row, segments)) = current.take() {
        rules.push(assemble_rule(row, segments)?);
    }
    Ok(rules)
}

/// Updates a recurring rule (base fields and full amount history) in one
/// transaction. Returns `true` if the rule existed.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn update(pool: &SqlitePool, rule: &RecurringRule) -> Result<bool, RepoError> {
    let id = id_to_i64(rule.id().get())?;
    let account_id = id_to_i64(rule.account_id().get())?;
    let amount = rule.base_amount().to_string();
    let kind = kind_to_text(rule.kind());
    let note = rule.note();
    let category_id = rule.category_id().map(|c| id_to_i64(c.get())).transpose()?;
    let start_date = date_to_text(rule.start_date());
    let (end_kind, end_date) = rule_end_to_columns(rule.end());
    let freq_unit = freq_unit_to_text(rule.frequency().unit());
    let freq_interval = i64::from(rule.frequency().interval());
    let segments = extra_segments(rule);

    let mut tx = pool.begin().await?;
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
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    // Replace the breakpoint set wholesale (simpler and correct for the small
    // segment counts here than diffing).
    sqlx::query!("DELETE FROM rule_amount WHERE rule_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for (effective_from, seg_amount) in &segments {
        sqlx::query!(
            "INSERT INTO rule_amount (rule_id, effective_from, amount) VALUES (?, ?, ?)",
            id,
            effective_from,
            seg_amount
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(true)
}

/// Whether a rule with `id` exists and belongs to `account_id`. Used to scope
/// per-occurrence commands to the calling account before they mutate.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn belongs_to(
    pool: &SqlitePool,
    id: RecurringRuleId,
    account_id: AccountId,
) -> Result<bool, RepoError> {
    let id = id_to_i64(id.get())?;
    let account_id = id_to_i64(account_id.get())?;
    let row = sqlx::query!(
        r#"SELECT 1 AS "one!" FROM recurring_rule WHERE id = ? AND account_id = ?"#,
        id,
        account_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
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
