//! Category-budget persistence (see `DESIGN.md` §12).

use sqlx::SqlitePool;
use talea_core::{AccountId, Budget, BudgetId, CategoryId, Money};

use crate::error::RepoError;
use crate::repo::map::{
    id_from_rowid, id_to_i64, month_from_columns, month_to_columns, opt_month_from_columns,
};

#[allow(clippy::too_many_arguments)] // mirrors the row columns; private helper
fn row_to_budget(
    id: i64,
    account_id: i64,
    category_id: i64,
    amount: &str,
    start_year: i64,
    start_month: i64,
    end_year: Option<i64>,
    end_month: Option<i64>,
) -> Result<Budget, RepoError> {
    let amount = Money::try_from_str(amount)
        .map_err(|e| RepoError::Corrupt(format!("invalid money {amount:?}: {e}")))?;
    let start = month_from_columns(start_year, start_month)?;
    let end = opt_month_from_columns(end_year, end_month)?;
    Budget::new(
        BudgetId::new(id_from_rowid(id)),
        AccountId::new(id_from_rowid(account_id)),
        CategoryId::new(id_from_rowid(category_id)),
        amount,
        start,
        end,
    )
    .map_err(|e| RepoError::corrupt(&e))
}

/// Inserts a validated draft budget and returns it with the assigned id.
///
/// # Errors
/// [`RepoError`] on a database error — including the `UNIQUE (account_id,
/// category_id)` constraint, which backstops the command layer's friendlier
/// duplicate check.
pub async fn insert(pool: &SqlitePool, draft: &Budget) -> Result<Budget, RepoError> {
    let account_id = id_to_i64(draft.account_id().get())?;
    let category_id = id_to_i64(draft.category_id().get())?;
    let amount = draft.amount().to_string();
    let (start_year, start_month) = month_to_columns(draft.start());
    let (end_year, end_month) = match draft.end() {
        Some(end) => {
            let (year, month) = month_to_columns(end);
            (Some(year), Some(month))
        }
        None => (None, None),
    };

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
    .fetch_one(pool)
    .await?;

    row_to_budget(
        rec.id,
        account_id,
        category_id,
        &amount,
        start_year,
        start_month,
        end_year,
        end_month,
    )
}

/// All budgets for an account, ordered by id.
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
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", account_id AS "account_id!", category_id AS "category_id!",
                  amount AS "amount!", start_year AS "start_year!",
                  start_month AS "start_month!", end_year, end_month
           FROM budget WHERE account_id = ? ORDER BY id"#,
        key
    )
    .fetch_all(executor)
    .await?;

    rows.into_iter()
        .map(|r| {
            row_to_budget(
                r.id,
                r.account_id,
                r.category_id,
                &r.amount,
                r.start_year,
                r.start_month,
                r.end_year,
                r.end_month,
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

/// Updates a budget in place. Returns `true` if a row was updated.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn update(pool: &SqlitePool, budget: &Budget) -> Result<bool, RepoError> {
    let id = id_to_i64(budget.id().get())?;
    let account_id = id_to_i64(budget.account_id().get())?;
    let category_id = id_to_i64(budget.category_id().get())?;
    let amount = budget.amount().to_string();
    let (start_year, start_month) = month_to_columns(budget.start());
    let (end_year, end_month) = match budget.end() {
        Some(end) => {
            let (year, month) = month_to_columns(end);
            (Some(year), Some(month))
        }
        None => (None, None),
    };

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
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
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
