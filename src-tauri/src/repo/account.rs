//! Account persistence.

use std::collections::HashMap;

use sqlx::{Sqlite, SqlitePool, Transaction};
use talea_core::{Account, AccountId, AccountKind, Currency, Month};

use crate::error::RepoError;
use crate::repo::map::{id_from_rowid, id_to_i64, money_from_text};

/// The DB token for an account kind (matches the `kind` CHECK and the domain's
/// serde tokens).
fn kind_to_text(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Normal => "normal",
        AccountKind::Summary => "summary",
    }
}

/// An `account` row as stored, every loader's `SELECT` reads it whole.
struct AccountRow {
    id: i64,
    name: String,
    icon: String,
    currency: String,
    opening_balance: String,
    anchor_year: i64,
    anchor_month: i64,
    kind: String,
}

impl AccountRow {
    /// Reconstructs the [`Account`] from this row plus the `members` loaded
    /// for it. Every column goes to the domain to validate, none is dropped as
    /// irrelevant: membership rows under a normal account, or a summary's
    /// opening balance, are refused as corrupt, where dropping them would hide
    /// them from every check while `is_member_of_any` still counted the rows.
    fn into_account(self, members: Vec<AccountId>) -> Result<Account, RepoError> {
        let currency = Currency::new(&self.currency).map_err(|e| RepoError::corrupt(&e))?;
        let opening = money_from_text(&self.opening_balance)?;
        let (year, month) = (self.anchor_year, self.anchor_month);
        let year = i32::try_from(year)
            .map_err(|_| RepoError::Corrupt(format!("anchor_year {year} out of range")))?;
        let month = u8::try_from(month)
            .map_err(|_| RepoError::Corrupt(format!("anchor_month {month} out of range")))?;
        let anchor = Month::new(year, month).map_err(|e| RepoError::corrupt(&e))?;
        let account_id = AccountId::new(id_from_rowid(self.id)?);
        let kind = match self.kind.as_str() {
            "normal" => AccountKind::Normal,
            "summary" => AccountKind::Summary,
            // The `kind` CHECK admits nothing else, so this is never long.
            other => {
                return Err(RepoError::Corrupt(format!(
                    "unknown account kind {other:?}"
                )))
            }
        };
        Account::from_parts(
            account_id, self.name, self.icon, currency, opening, anchor, kind, members,
        )
        .map_err(|e| RepoError::corrupt(&e))
    }
}

/// Reads a summary account's member ids, in insertion order.
async fn members_for<'e, E>(executor: E, summary_id: i64) -> Result<Vec<AccountId>, RepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let rows = sqlx::query!(
        r#"SELECT member_account_id AS "member_account_id!"
           FROM account_member WHERE summary_account_id = ? ORDER BY rowid"#,
        summary_id
    )
    .fetch_all(executor)
    .await?;
    rows.into_iter()
        .map(|r| id_from_rowid(r.member_account_id).map(AccountId::new))
        .collect()
}

/// Replaces a summary account's membership set within an existing transaction.
async fn set_members(
    tx: &mut Transaction<'_, Sqlite>,
    summary_id: i64,
    members: &[AccountId],
) -> Result<(), RepoError> {
    sqlx::query!(
        "DELETE FROM account_member WHERE summary_account_id = ?",
        summary_id
    )
    .execute(&mut **tx)
    .await?;
    for member in members {
        let member_id = id_to_i64(member.get())?;
        sqlx::query!(
            "INSERT INTO account_member (summary_account_id, member_account_id) VALUES (?, ?)",
            summary_id,
            member_id
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Inserts a validated draft account and returns it with the assigned id, in
/// the caller's transaction, so it can check what the insert depends on (a
/// summary's members) in the same one; a summary's membership is written in it
/// too. Taking a transaction, not any connection, keeps the account row and
/// its membership rows one write.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn insert(
    tx: &mut Transaction<'_, Sqlite>,
    draft: &Account,
) -> Result<Account, RepoError> {
    let name = draft.name();
    let icon = draft.icon();
    let currency = draft.currency().code();
    let opening = draft.opening_balance().to_string();
    let year = i64::from(draft.anchor().year());
    let month = i64::from(draft.anchor().month());
    let kind = kind_to_text(draft.kind());

    let rec = sqlx::query!(
        r#"INSERT INTO account (name, icon, currency, opening_balance, anchor_year, anchor_month, kind)
           VALUES (?, ?, ?, ?, ?, ?, ?)
           RETURNING id AS "id!""#,
        name,
        icon,
        currency,
        opening,
        year,
        month,
        kind
    )
    .fetch_one(&mut **tx)
    .await?;
    if draft.kind() == AccountKind::Summary {
        set_members(tx, rec.id, draft.members()).await?;
    }

    AccountRow {
        id: rec.id,
        name: name.to_owned(),
        icon: icon.to_owned(),
        currency: currency.to_owned(),
        opening_balance: opening,
        anchor_year: year,
        anchor_month: month,
        kind: kind.to_owned(),
    }
    .into_account(draft.members().to_vec())
}

/// Lists all accounts, ordered by id.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn list(pool: &SqlitePool) -> Result<Vec<Account>, RepoError> {
    let mut conn = pool.acquire().await?;
    list_on(&mut conn).await
}

/// [`list`] on a given connection, so restore can validate a backup with it
/// inside its transaction. A concrete connection rather than a generic
/// `Acquire`: its two queries need a reborrow, and the generic form defeats the
/// `Send` inference the async commands depend on.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn list_on(conn: &mut sqlx::SqliteConnection) -> Result<Vec<Account>, RepoError> {
    let rows = sqlx::query_as!(
        AccountRow,
        r#"SELECT id AS "id!", name AS "name!", icon AS "icon!", currency AS "currency!",
                  opening_balance AS "opening_balance!", anchor_year AS "anchor_year!",
                  anchor_month AS "anchor_month!", kind AS "kind!"
           FROM account ORDER BY id"#
    )
    .fetch_all(&mut *conn)
    .await?;

    // Load all memberships in one pass and group by summary, avoiding N+1.
    let links = sqlx::query!(
        r#"SELECT summary_account_id AS "summary_account_id!",
                  member_account_id AS "member_account_id!"
           FROM account_member ORDER BY rowid"#
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut members_by_summary: HashMap<i64, Vec<AccountId>> = HashMap::new();
    for link in links {
        members_by_summary
            .entry(link.summary_account_id)
            .or_default()
            .push(AccountId::new(id_from_rowid(link.member_account_id)?));
    }

    rows.into_iter()
        .map(|row| {
            let members = members_by_summary.remove(&row.id).unwrap_or_default();
            row.into_account(members)
        })
        .collect()
}

/// Fetches one account by id, **without** populating a summary's members (so it
/// stays generic over the executor and usable inside a transaction): a summary
/// comes back with none. Use [`get_full`] when a summary's members are needed.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn get<'e, E>(executor: E, id: AccountId) -> Result<Option<Account>, RepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let key = id_to_i64(id.get())?;
    let row = sqlx::query_as!(
        AccountRow,
        r#"SELECT id AS "id!", name AS "name!", icon AS "icon!", currency AS "currency!",
                  opening_balance AS "opening_balance!", anchor_year AS "anchor_year!",
                  anchor_month AS "anchor_month!", kind AS "kind!"
           FROM account WHERE id = ?"#,
        key
    )
    .fetch_optional(executor)
    .await?;
    row.map(|row| row.into_account(Vec::new())).transpose()
}

/// Fetches the accounts `ids` name that exist, in one query, **without** a
/// summary's members (as [`get`]): what a summary's membership check needs of
/// its members, read while the caller holds the write lock.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn get_many<'e, E>(executor: E, ids: &[AccountId]) -> Result<Vec<Account>, RepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let keys = ids
        .iter()
        .map(|id| id_to_i64(id.get()))
        .collect::<Result<Vec<_>, _>>()?;
    let keys = serde_json::Value::from(keys).to_string();
    let rows = sqlx::query_as!(
        AccountRow,
        r#"SELECT id AS "id!", name AS "name!", icon AS "icon!", currency AS "currency!",
                  opening_balance AS "opening_balance!", anchor_year AS "anchor_year!",
                  anchor_month AS "anchor_month!", kind AS "kind!"
           FROM account WHERE id IN (SELECT value FROM json_each(?))"#,
        keys
    )
    .fetch_all(executor)
    .await?;
    rows.into_iter()
        .map(|row| row.into_account(Vec::new()))
        .collect()
}

/// Fetches one account by id with a summary's members populated.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn get_full(pool: &SqlitePool, id: AccountId) -> Result<Option<Account>, RepoError> {
    let key = id_to_i64(id.get())?;
    let row = sqlx::query_as!(
        AccountRow,
        r#"SELECT id AS "id!", name AS "name!", icon AS "icon!", currency AS "currency!",
                  opening_balance AS "opening_balance!", anchor_year AS "anchor_year!",
                  anchor_month AS "anchor_month!", kind AS "kind!"
           FROM account WHERE id = ?"#,
        key
    )
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    let members = if row.kind == "summary" {
        members_for(pool, row.id).await?
    } else {
        Vec::new()
    };
    row.into_account(members).map(Some)
}

/// Whether `id` is a member of any summary account.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn is_member_of_any<'e, E>(executor: E, id: AccountId) -> Result<bool, RepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let key = id_to_i64(id.get())?;
    let row = sqlx::query!(
        r#"SELECT EXISTS(SELECT 1 FROM account_member WHERE member_account_id = ?) AS "exists!""#,
        key
    )
    .fetch_one(executor)
    .await?;
    Ok(row.exists != 0)
}

/// Updates an account in place, replacing its membership set, in the caller's
/// transaction, so it can check in the same one what the update depends on.
/// Returns `true` if a row was updated.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn update(
    tx: &mut Transaction<'_, Sqlite>,
    account: &Account,
) -> Result<bool, RepoError> {
    let id = id_to_i64(account.id().get())?;
    let name = account.name();
    let icon = account.icon();
    let currency = account.currency().code();
    let opening = account.opening_balance().to_string();
    let year = i64::from(account.anchor().year());
    let month = i64::from(account.anchor().month());
    let kind = kind_to_text(account.kind());

    let result = sqlx::query!(
        r#"UPDATE account
           SET name = ?, icon = ?, currency = ?, opening_balance = ?,
               anchor_year = ?, anchor_month = ?, kind = ?
           WHERE id = ?"#,
        name,
        icon,
        currency,
        opening,
        year,
        month,
        kind,
        id
    )
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(false);
    }
    // Only summary accounts have membership rows; an account's kind is fixed, so
    // a normal account never needs to touch `account_member`.
    if account.kind() == AccountKind::Summary {
        set_members(tx, id, account.members()).await?;
    }
    Ok(true)
}

/// Deletes an account (cascading its entries, rules, and memberships), in the
/// caller's transaction, so it can check in the same one whether the account
/// may go. Returns `true` if a row was deleted.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn delete(tx: &mut Transaction<'_, Sqlite>, id: AccountId) -> Result<bool, RepoError> {
    let key = id_to_i64(id.get())?;
    let result = sqlx::query!("DELETE FROM account WHERE id = ?", key)
        .execute(&mut **tx)
        .await?;
    Ok(result.rows_affected() > 0)
}
