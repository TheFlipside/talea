//! Entry persistence.

use sqlx::SqlitePool;
use talea_core::{AccountId, CategoryId, Entry, EntryId, Money};

use crate::error::RepoError;
use crate::repo::map::{
    date_from_text, date_to_text, id_from_rowid, id_to_i64, kind_from_text, kind_to_text,
};

#[allow(clippy::too_many_arguments)] // mirrors the row columns; private helper
fn row_to_entry(
    id: i64,
    account_id: i64,
    amount: &str,
    kind: &str,
    date: &str,
    note: Option<String>,
    category_id: Option<i64>,
) -> Result<Entry, RepoError> {
    let amount = Money::try_from_str(amount)
        .map_err(|e| RepoError::Corrupt(format!("invalid money {amount:?}: {e}")))?;
    let kind = kind_from_text(kind)?;
    let date = date_from_text(date)?;
    let category = category_id.map(|c| CategoryId::new(id_from_rowid(c)));
    Entry::new(
        EntryId::new(id_from_rowid(id)),
        AccountId::new(id_from_rowid(account_id)),
        amount,
        kind,
        date,
        note,
        category,
    )
    .map_err(|e| RepoError::corrupt(&e))
}

/// Inserts a validated draft entry and returns it with the assigned id.
///
/// Generic over the executor so it can run on the pool or inside a transaction
/// (e.g. the atomic "detach an occurrence" path, which inserts a skip and this
/// entry together).
///
/// # Errors
/// [`RepoError`] on a database error (e.g. a non-existent `account_id`).
pub async fn insert<'e, E>(executor: E, draft: &Entry) -> Result<Entry, RepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let account_id = id_to_i64(draft.account_id().get())?;
    let amount = draft.amount().to_string();
    let kind = kind_to_text(draft.kind());
    let date = date_to_text(draft.date());
    let note = draft.note();
    let category_id = draft
        .category_id()
        .map(|c| id_to_i64(c.get()))
        .transpose()?;

    let rec = sqlx::query!(
        r#"INSERT INTO entry (account_id, amount, kind, date, note, category_id)
           VALUES (?, ?, ?, ?, ?, ?)
           RETURNING id AS "id!""#,
        account_id,
        amount,
        kind,
        date,
        note,
        category_id
    )
    .fetch_one(executor)
    .await?;

    row_to_entry(
        rec.id,
        account_id,
        &amount,
        kind,
        &date,
        draft.note().map(str::to_owned),
        category_id,
    )
}

/// All entries for an account, ordered by date then id.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn for_account<'e, E>(executor: E, account_id: AccountId) -> Result<Vec<Entry>, RepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let key = id_to_i64(account_id.get())?;
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", account_id AS "account_id!", amount AS "amount!",
                  kind AS "kind!", date AS "date!", note, category_id
           FROM entry WHERE account_id = ? ORDER BY date, id"#,
        key
    )
    .fetch_all(executor)
    .await?;

    rows.into_iter()
        .map(|r| {
            row_to_entry(
                r.id,
                r.account_id,
                &r.amount,
                &r.kind,
                &r.date,
                r.note,
                r.category_id,
            )
        })
        .collect()
}

/// Updates an entry in place. Returns `true` if a row was updated.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn update(pool: &SqlitePool, entry: &Entry) -> Result<bool, RepoError> {
    let id = id_to_i64(entry.id().get())?;
    let account_id = id_to_i64(entry.account_id().get())?;
    let amount = entry.amount().to_string();
    let kind = kind_to_text(entry.kind());
    let date = date_to_text(entry.date());
    let note = entry.note();
    let category_id = entry
        .category_id()
        .map(|c| id_to_i64(c.get()))
        .transpose()?;

    let result = sqlx::query!(
        r#"UPDATE entry
           SET account_id = ?, amount = ?, kind = ?, date = ?, note = ?, category_id = ?
           WHERE id = ?"#,
        account_id,
        amount,
        kind,
        date,
        note,
        category_id,
        id
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Deletes an entry. Returns `true` if a row was deleted.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn delete(pool: &SqlitePool, id: EntryId) -> Result<bool, RepoError> {
    let key = id_to_i64(id.get())?;
    let result = sqlx::query!("DELETE FROM entry WHERE id = ?", key)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
