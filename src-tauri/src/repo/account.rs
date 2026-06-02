//! Account persistence.

use std::collections::HashMap;

use sqlx::SqlitePool;
use talea_core::{Account, AccountId, AccountKind, Currency, Money, Month};

use crate::error::RepoError;
use crate::repo::map::{id_from_rowid, id_to_i64};

/// The DB token for an account kind (matches the `kind` CHECK and the domain's
/// serde tokens).
fn kind_to_text(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Normal => "normal",
        AccountKind::Summary => "summary",
    }
}

/// Reconstructs an [`Account`] from its row plus its already-loaded `members`
/// (empty for a normal account). Dispatches on `kind` so a summary is built
/// through the summary constructor (which fixes its opening balance to zero).
#[allow(clippy::too_many_arguments)] // mirrors the row columns; private helper
fn row_to_account(
    id: i64,
    name: String,
    icon: String,
    currency: &str,
    opening_balance: &str,
    anchor_year: i64,
    anchor_month: i64,
    kind: &str,
    members: Vec<AccountId>,
) -> Result<Account, RepoError> {
    let currency = Currency::new(currency).map_err(|e| RepoError::corrupt(&e))?;
    let opening = Money::try_from_str(opening_balance)
        .map_err(|e| RepoError::Corrupt(format!("invalid money {opening_balance:?}: {e}")))?;
    let year = i32::try_from(anchor_year)
        .map_err(|_| RepoError::Corrupt(format!("anchor_year {anchor_year} out of range")))?;
    let month = u8::try_from(anchor_month)
        .map_err(|_| RepoError::Corrupt(format!("anchor_month {anchor_month} out of range")))?;
    let anchor = Month::new(year, month).map_err(|e| RepoError::corrupt(&e))?;
    let account_id = AccountId::new(id_from_rowid(id));
    match kind {
        "normal" => Account::new(account_id, name, icon, currency, opening, anchor)
            .map_err(|e| RepoError::corrupt(&e)),
        "summary" => Account::new_summary(account_id, name, icon, currency, anchor, members)
            .map_err(|e| RepoError::corrupt(&e)),
        other => Err(RepoError::Corrupt(format!(
            "unknown account kind {other:?}"
        ))),
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
    Ok(rows
        .into_iter()
        .map(|r| AccountId::new(id_from_rowid(r.member_account_id)))
        .collect())
}

/// Replaces a summary account's membership set within an existing transaction.
async fn set_members(
    tx: &mut sqlx::SqliteConnection,
    summary_id: i64,
    members: &[AccountId],
) -> Result<(), RepoError> {
    sqlx::query!(
        "DELETE FROM account_member WHERE summary_account_id = ?",
        summary_id
    )
    .execute(&mut *tx)
    .await?;
    for member in members {
        let member_id = id_to_i64(member.get())?;
        sqlx::query!(
            "INSERT INTO account_member (summary_account_id, member_account_id) VALUES (?, ?)",
            summary_id,
            member_id
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// Inserts a validated draft account and returns it with the assigned id.
/// Membership (for a summary account) is written in the same transaction.
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
    let kind = kind_to_text(draft.kind());

    let mut tx = pool.begin().await?;
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
    .fetch_one(&mut *tx)
    .await?;
    if draft.kind() == AccountKind::Summary {
        set_members(&mut tx, rec.id, draft.members()).await?;
    }
    tx.commit().await?;

    row_to_account(
        rec.id,
        draft.name().to_owned(),
        draft.icon().to_owned(),
        draft.currency().code(),
        &draft.opening_balance().to_string(),
        i64::from(draft.anchor().year()),
        i64::from(draft.anchor().month()),
        kind,
        draft.members().to_vec(),
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
                  anchor_month AS "anchor_month!", kind AS "kind!"
           FROM account ORDER BY id"#
    )
    .fetch_all(pool)
    .await?;

    // Load all memberships in one pass and group by summary, avoiding N+1.
    let links = sqlx::query!(
        r#"SELECT summary_account_id AS "summary_account_id!",
                  member_account_id AS "member_account_id!"
           FROM account_member ORDER BY rowid"#
    )
    .fetch_all(pool)
    .await?;
    let mut members_by_summary: HashMap<i64, Vec<AccountId>> = HashMap::new();
    for link in links {
        members_by_summary
            .entry(link.summary_account_id)
            .or_default()
            .push(AccountId::new(id_from_rowid(link.member_account_id)));
    }

    rows.into_iter()
        .map(|r| {
            let members = members_by_summary.remove(&r.id).unwrap_or_default();
            row_to_account(
                r.id,
                r.name,
                r.icon,
                &r.currency,
                &r.opening_balance,
                r.anchor_year,
                r.anchor_month,
                &r.kind,
                members,
            )
        })
        .collect()
}

/// Fetches one account by id, **without** populating a summary's members (so it
/// stays generic over the executor and usable inside a transaction). Callers
/// inside the per-account snapshot only ever load normal accounts; use
/// [`get_full`] when a summary's members are needed.
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
                  anchor_month AS "anchor_month!", kind AS "kind!"
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
            &r.kind,
            Vec::new(),
        )
    })
    .transpose()
}

/// Fetches one account by id with a summary's members populated.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn get_full(pool: &SqlitePool, id: AccountId) -> Result<Option<Account>, RepoError> {
    let key = id_to_i64(id.get())?;
    let row = sqlx::query!(
        r#"SELECT id AS "id!", name AS "name!", icon AS "icon!", currency AS "currency!",
                  opening_balance AS "opening_balance!", anchor_year AS "anchor_year!",
                  anchor_month AS "anchor_month!", kind AS "kind!"
           FROM account WHERE id = ?"#,
        key
    )
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else {
        return Ok(None);
    };
    let members = if r.kind == "summary" {
        members_for(pool, r.id).await?
    } else {
        Vec::new()
    };
    row_to_account(
        r.id,
        r.name,
        r.icon,
        &r.currency,
        &r.opening_balance,
        r.anchor_year,
        r.anchor_month,
        &r.kind,
        members,
    )
    .map(Some)
}

/// Whether `id` is a member of any summary account.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn is_member_of_any(pool: &SqlitePool, id: AccountId) -> Result<bool, RepoError> {
    let key = id_to_i64(id.get())?;
    let row = sqlx::query!(
        r#"SELECT EXISTS(SELECT 1 FROM account_member WHERE member_account_id = ?) AS "exists!""#,
        key
    )
    .fetch_one(pool)
    .await?;
    Ok(row.exists != 0)
}

/// Updates an account in place, replacing its membership set. Returns `true` if
/// a row was updated.
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
    let kind = kind_to_text(account.kind());

    let mut tx = pool.begin().await?;
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
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(false);
    }
    // Only summary accounts have membership rows; an account's kind is fixed, so
    // a normal account never needs to touch `account_member`.
    if account.kind() == AccountKind::Summary {
        set_members(&mut tx, id, account.members()).await?;
    }
    tx.commit().await?;

    Ok(true)
}

/// Deletes an account (cascading its entries, rules, and memberships). Returns
/// `true` if a row was deleted.
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
