//! Category-budget persistence (see `DESIGN.md` §12).

use sqlx::SqlitePool;
use talea_core::{AccountId, Budget, BudgetId, BudgetSegment, CategoryId};

use crate::error::RepoError;
use crate::repo::map::{
    id_from_rowid, id_to_i64, money_from_text, month_from_columns, month_to_columns,
    opt_month_from_columns,
};

/// The `budget` columns, as read for [`assemble_budget`].
struct BudgetRow {
    id: i64,
    account_id: i64,
    category_id: i64,
    amount: String,
    start_year: i64,
    start_month: i64,
    end_year: Option<i64>,
    end_month: Option<i64>,
}

/// One `budget_amount` breakpoint as raw columns.
type SegmentRow = (i64, i64, String);

fn assemble_budget(row: BudgetRow, extra: Vec<SegmentRow>) -> Result<Budget, RepoError> {
    let BudgetRow {
        id,
        account_id,
        category_id,
        amount,
        start_year,
        start_month,
        end_year,
        end_month,
    } = row;
    let start = month_from_columns(start_year, start_month)?;
    let end = opt_month_from_columns(end_year, end_month)?;

    // Base segment (at the budget's start) followed by the stored breakpoints.
    let mut amounts = vec![BudgetSegment::new(start, money_from_text(&amount)?)];
    for (year, month, seg_amount) in extra {
        amounts.push(BudgetSegment::new(
            month_from_columns(year, month)?,
            money_from_text(&seg_amount)?,
        ));
    }

    Budget::new_with_amounts(
        BudgetId::new(id_from_rowid(id)?),
        AccountId::new(id_from_rowid(account_id)?),
        CategoryId::new(id_from_rowid(category_id)?),
        amounts,
        start,
        end,
    )
    .map_err(|e| RepoError::corrupt(&e))
}

/// The budget's amount breakpoints beyond the base (the segments after the
/// first), as `(year, month, amount)` ready for `budget_amount`.
fn extra_segments(budget: &Budget) -> Vec<SegmentRow> {
    budget
        .amounts()
        .iter()
        .skip(1)
        .map(|seg| {
            let (year, month) = month_to_columns(seg.effective_from());
            (year, month, seg.amount().to_string())
        })
        .collect()
}

/// Splits a budget's end month into its nullable `(year, month)` columns.
fn end_columns(budget: &Budget) -> (Option<i64>, Option<i64>) {
    match budget.end() {
        Some(end) => {
            let (year, month) = month_to_columns(end);
            (Some(year), Some(month))
        }
        None => (None, None),
    }
}

/// Inserts a validated draft budget (and any amount breakpoints) in one
/// transaction, returning it with the assigned id.
///
/// # Errors
/// [`RepoError`] on a database error — including the `UNIQUE (account_id,
/// category_id)` constraint, which backstops the command layer's friendlier
/// duplicate check.
pub async fn insert(pool: &SqlitePool, draft: &Budget) -> Result<Budget, RepoError> {
    let account_id = id_to_i64(draft.account_id().get())?;
    let category_id = id_to_i64(draft.category_id().get())?;
    let amount = draft.base_amount().to_string();
    let (start_year, start_month) = month_to_columns(draft.start());
    let (end_year, end_month) = end_columns(draft);
    let segments = extra_segments(draft);

    let mut tx = pool.begin().await?;
    let rec = sqlx::query!(
        r#"INSERT INTO budget
               (account_id, category_id, amount, start_year, start_month,
                end_year, end_month)
           VALUES (?, ?, ?, ?, ?, ?, ?)
           RETURNING id AS "id!""#,
        account_id,
        category_id,
        amount,
        start_year,
        start_month,
        end_year,
        end_month
    )
    .fetch_one(&mut *tx)
    .await?;

    for (year, month, seg_amount) in &segments {
        sqlx::query!(
            r#"INSERT INTO budget_amount
                   (budget_id, effective_year, effective_month, amount)
               VALUES (?, ?, ?, ?)"#,
            rec.id,
            year,
            month,
            seg_amount
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    assemble_budget(
        BudgetRow {
            id: rec.id,
            account_id,
            category_id,
            amount,
            start_year,
            start_month,
            end_year,
            end_month,
        },
        segments,
    )
}

/// All budgets for an account (with their amount history), ordered by id.
///
/// Generic over the executor so it can join the caller's read transaction —
/// `reserved` depends on budgets *and* entries, so both must come from one
/// consistent snapshot.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn for_account<'e, E>(
    executor: E,
    account_id: AccountId,
) -> Result<Vec<Budget>, RepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let key = id_to_i64(account_id.get())?;
    // One statement, so each budget arrives with its breakpoints from a single
    // consistent read, one row per budget, its breakpoints as one JSON array in
    // month order. A join would repeat the budget's columns on every breakpoint.
    let rows = sqlx::query!(
        r#"SELECT b.id AS "id!", b.account_id AS "account_id!", b.category_id AS "category_id!",
                  b.amount AS "amount!", b.start_year AS "start_year!",
                  b.start_month AS "start_month!", b.end_year, b.end_month,
                  (SELECT json_group_array(
                              json_array(ba.effective_year, ba.effective_month, ba.amount)
                              ORDER BY ba.effective_year, ba.effective_month)
                   FROM budget_amount ba WHERE ba.budget_id = b.id) AS "segments!: String"
           FROM budget b
           WHERE b.account_id = ?
           ORDER BY b.id"#,
        key
    )
    .fetch_all(executor)
    .await?;

    rows.into_iter()
        .map(|r| {
            let segments: Vec<SegmentRow> = serde_json::from_str(&r.segments)
                .map_err(|_| RepoError::Corrupt("invalid amount history".to_owned()))?;
            assemble_budget(
                BudgetRow {
                    id: r.id,
                    account_id: r.account_id,
                    category_id: r.category_id,
                    amount: r.amount,
                    start_year: r.start_year,
                    start_month: r.start_month,
                    end_year: r.end_year,
                    end_month: r.end_month,
                },
                segments,
            )
        })
        .collect()
}

/// Whether `account_id` already budgets `category_id`.
///
/// `excluding` is the budget being updated, so an unchanged row never collides
/// with itself.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn exists_for_category(
    pool: &SqlitePool,
    account_id: AccountId,
    category_id: CategoryId,
    excluding: Option<BudgetId>,
) -> Result<bool, RepoError> {
    let account_key = id_to_i64(account_id.get())?;
    let category_key = id_to_i64(category_id.get())?;
    // `-1` can never be a rowid, so "exclude nothing" needs no second query.
    let exclude = excluding
        .map(|id| id_to_i64(id.get()))
        .transpose()?
        .unwrap_or(-1);

    let row = sqlx::query!(
        r#"SELECT 1 AS "one!" FROM budget
           WHERE account_id = ? AND category_id = ? AND id <> ?"#,
        account_key,
        category_key,
        exclude
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.is_some())
}

/// Updates a budget (base fields and full amount history) in one transaction.
/// Returns `true` if the budget existed.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn update(pool: &SqlitePool, budget: &Budget) -> Result<bool, RepoError> {
    let id = id_to_i64(budget.id().get())?;
    let account_id = id_to_i64(budget.account_id().get())?;
    let category_id = id_to_i64(budget.category_id().get())?;
    let amount = budget.base_amount().to_string();
    let (start_year, start_month) = month_to_columns(budget.start());
    let (end_year, end_month) = end_columns(budget);
    let segments = extra_segments(budget);

    let mut tx = pool.begin().await?;
    let result = sqlx::query!(
        r#"UPDATE budget
           SET account_id = ?, category_id = ?, amount = ?, start_year = ?,
               start_month = ?, end_year = ?, end_month = ?
           WHERE id = ?"#,
        account_id,
        category_id,
        amount,
        start_year,
        start_month,
        end_year,
        end_month,
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
    sqlx::query!("DELETE FROM budget_amount WHERE budget_id = ?", id)
        .execute(&mut *tx)
        .await?;
    for (year, month, seg_amount) in &segments {
        sqlx::query!(
            r#"INSERT INTO budget_amount
                   (budget_id, effective_year, effective_month, amount)
               VALUES (?, ?, ?, ?)"#,
            id,
            year,
            month,
            seg_amount
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(true)
}

/// Deletes a budget. Returns `true` if a row was deleted.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn delete(pool: &SqlitePool, id: BudgetId) -> Result<bool, RepoError> {
    let key = id_to_i64(id.get())?;
    let result = sqlx::query!("DELETE FROM budget WHERE id = ?", key)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
