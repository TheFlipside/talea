//! Category persistence (global, shared across accounts).

use sqlx::SqlitePool;
use talea_core::{Category, CategoryId};

use crate::error::RepoError;
use crate::repo::map::{icon_from_columns, icon_to_columns, id_from_rowid, id_to_i64};

fn row_to_category(
    id: i64,
    label: String,
    icon_kind: &str,
    icon_value: String,
) -> Result<Category, RepoError> {
    let icon = icon_from_columns(icon_kind, icon_value)?;
    Category::new(CategoryId::new(id_from_rowid(id)), label, icon)
        .map_err(|e| RepoError::corrupt(&e))
}

/// Inserts a validated draft category and returns it with the assigned id.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn insert(pool: &SqlitePool, draft: &Category) -> Result<Category, RepoError> {
    let label = draft.label();
    let (icon_kind, icon_value) = icon_to_columns(draft.icon());

    let rec = sqlx::query!(
        r#"INSERT INTO category (label, icon_kind, icon_value)
           VALUES (?, ?, ?)
           RETURNING id AS "id!""#,
        label,
        icon_kind,
        icon_value
    )
    .fetch_one(pool)
    .await?;

    row_to_category(
        rec.id,
        draft.label().to_owned(),
        icon_kind,
        icon_value.to_owned(),
    )
}

/// Lists all categories, ordered by id.
///
/// # Errors
/// [`RepoError`] on a database error or a row that fails domain validation.
pub async fn list(pool: &SqlitePool) -> Result<Vec<Category>, RepoError> {
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", label AS "label!", icon_kind AS "icon_kind!",
                  icon_value AS "icon_value!"
           FROM category ORDER BY id"#
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| row_to_category(r.id, r.label, &r.icon_kind, r.icon_value))
        .collect()
}

/// Updates a category in place. Returns `true` if a row was updated.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn update(pool: &SqlitePool, category: &Category) -> Result<bool, RepoError> {
    let id = id_to_i64(category.id().get())?;
    let label = category.label();
    let (icon_kind, icon_value) = icon_to_columns(category.icon());

    let result = sqlx::query!(
        r#"UPDATE category SET label = ?, icon_kind = ?, icon_value = ? WHERE id = ?"#,
        label,
        icon_kind,
        icon_value,
        id
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Deletes a category (entries/rules referencing it have their `category_id`
/// set to NULL). Returns `true` if a row was deleted.
///
/// # Errors
/// [`RepoError`] on a database error.
pub async fn delete(pool: &SqlitePool, id: CategoryId) -> Result<bool, RepoError> {
    let key = id_to_i64(id.get())?;
    let result = sqlx::query!("DELETE FROM category WHERE id = ?", key)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
