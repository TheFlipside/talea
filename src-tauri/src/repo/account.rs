//! Account persistence.

use sqlx::SqlitePool;
use talea_core::{Account, AccountId, Currency, Money, Month};

use crate::error::RepoError;
use crate::repo::map::{id_from_rowid, id_to_i64};

#[allow(clippy::too_many_arguments)] // mirrors the row columns; private helper
fn row_to_account(
    id: i64,
    name: String,
    icon: String,
    currency: &str,
    opening_balance: &str,
    anchor_year: i64,
    anchor_month: i64,
) -> Result<Account, RepoError> {
    let currency = Currency::new(currency).map_err(|e| RepoError::corrupt(&e))?;
    let opening = Money::try_from_str(opening_balance)
        .map_err(|e| RepoError::Corrupt(format!("invalid money {opening_balance:?}: {e}")))?;
    let year = i32::try_from(anchor_year)
        .map_err(|_| RepoError::Corrupt(format!("anchor_year {anchor_year} out of range")))?;
    let month = u8::try_from(anchor_month)
        .map_err(|_| RepoError::Corrupt(format!("anchor_month {anchor_month} out of range")))?;
    let anchor = Month::new(year, month).map_err(|e| RepoError::corrupt(&e))?;
    Account::new(
        AccountId::new(id_from_rowid(id)),
        name,
        icon,
        currency,
        opening,
        anchor,
    )
    .map_err(|e| RepoError::corrupt(&e))
}

/// Inserts a validated draft account and returns it with the assigned id.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn insert(pool: &SqlitePool, draft: &Account) -> Result<Account, RepoError> {
    let name = draft.name();
    let icon = draft.icon();
    let currency = draft.currency().code();
    let opening = draft.opening_balance().to_string();
    let year = i64::from(draft.anchor().year());
    let month = i64::from(draft.anchor().month());

    let rec = sqlx::query!(
        r#"INSERT INTO account (name, icon, currency, opening_balance, anchor_year, anchor_month)
           VALUES (?, ?, ?, ?, ?, ?)
           RETURNING id AS "id!""#,
        name,
        icon,
        currency,
        opening,
        year,
        month
    )
    .fetch_one(pool)
    .await?;

    row_to_account(
        rec.id,
        draft.name().to_owned(),
        draft.icon().to_owned(),
        draft.currency().code(),
        &draft.opening_balance().to_string(),
        i64::from(draft.anchor().year()),
        i64::from(draft.anchor().month()),
    )
}

/// Lists all accounts, ordered by id.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn list(pool: &SqlitePool) -> Result<Vec<Account>, RepoError> {
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", name AS "name!", icon AS "icon!", currency AS "currency!",
                  opening_balance AS "opening_balance!", anchor_year AS "anchor_year!",
                  anchor_month AS "anchor_month!"
           FROM account ORDER BY id"#
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            row_to_account(
                r.id,
                r.name,
                r.icon,
                &r.currency,
                &r.opening_balance,
                r.anchor_year,
                r.anchor_month,
            )
        })
        .collect()
}

/// Fetches one account by id.
///
/// Generic over the executor so it can run on a pool or inside a transaction
/// (the ledger commands read account + entries + rules in one snapshot).
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn get<'e, E>(executor: E, id: AccountId) -> Result<Option<Account>, RepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let key = id_to_i64(id.get())?;
    let row = sqlx::query!(
        r#"SELECT id AS "id!", name AS "name!", icon AS "icon!", currency AS "currency!",
                  opening_balance AS "opening_balance!", anchor_year AS "anchor_year!",
                  anchor_month AS "anchor_month!"
           FROM account WHERE id = ?"#,
        key
    )
    .fetch_optional(executor)
    .await?;

    row.map(|r| {
        row_to_account(
            r.id,
            r.name,
            r.icon,
            &r.currency,
            &r.opening_balance,
            r.anchor_year,
            r.anchor_month,
        )
    })
    .transpose()
}

/// Updates an account in place. Returns `true` if a row was updated.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn update(pool: &SqlitePool, account: &Account) -> Result<bool, RepoError> {
    let id = id_to_i64(account.id().get())?;
    let name = account.name();
    let icon = account.icon();
    let currency = account.currency().code();
    let opening = account.opening_balance().to_string();
    let year = i64::from(account.anchor().year());
    let month = i64::from(account.anchor().month());

    let result = sqlx::query!(
        r#"UPDATE account
           SET name = ?, icon = ?, currency = ?, opening_balance = ?,
               anchor_year = ?, anchor_month = ?
           WHERE id = ?"#,
        name,
        icon,
        currency,
        opening,
        year,
        month,
        id
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Deletes an account (cascading its entries and rules). Returns `true` if a
/// row was deleted.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn delete(pool: &SqlitePool, id: AccountId) -> Result<bool, RepoError> {
    let key = id_to_i64(id.get())?;
    let result = sqlx::query!("DELETE FROM account WHERE id = ?", key)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
