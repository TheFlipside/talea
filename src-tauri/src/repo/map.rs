//! Shared row↔domain conversion helpers.
//!
//! These translate between the database's TEXT/INTEGER columns and the domain's
//! types. The token strings here MUST match the `CHECK` constraints in the
//! migration and the domain's serde `rename_all = "snake_case"` output.

use talea_core::{CategoryIcon, EntryKind, FreqUnit, RuleEnd};
use time::Date;

use crate::error::RepoError;

/// Formats a date as ISO `YYYY-MM-DD` for storage.
pub(crate) fn date_to_text(date: Date) -> String {
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    date.format(fmt)
        .expect("formatting a valid date as ISO cannot fail")
}

/// Parses an ISO `YYYY-MM-DD` date from storage.
pub(crate) fn date_from_text(text: &str) -> Result<Date, RepoError> {
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    Date::parse(text, fmt).map_err(|e| RepoError::Corrupt(format!("invalid date {text:?}: {e}")))
}

/// Maps a typed id's raw value to the `i64` `SQLite` binds.
///
/// # Errors
/// [`RepoError::InvalidId`] if `raw` does not fit `i64` (no such row could
/// exist; surfaced as a validation error rather than masked as a missing row or
/// an opaque foreign-key failure).
pub(crate) fn id_to_i64(raw: u64) -> Result<i64, RepoError> {
    i64::try_from(raw).map_err(|_| RepoError::InvalidId(raw))
}

/// Maps a `SQLite` rowid back to a typed id's raw value.
pub(crate) fn id_from_rowid(rowid: i64) -> u64 {
    u64::try_from(rowid).unwrap_or(0)
}

pub(crate) fn kind_to_text(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Income => "income",
        EntryKind::Expense => "expense",
    }
}

pub(crate) fn kind_from_text(text: &str) -> Result<EntryKind, RepoError> {
    match text {
        "income" => Ok(EntryKind::Income),
        "expense" => Ok(EntryKind::Expense),
        other => Err(RepoError::Corrupt(format!("invalid entry kind {other:?}"))),
    }
}

pub(crate) fn freq_unit_to_text(unit: FreqUnit) -> &'static str {
    match unit {
        FreqUnit::Weekly => "weekly",
        FreqUnit::Monthly => "monthly",
        FreqUnit::Yearly => "yearly",
    }
}

pub(crate) fn freq_unit_from_text(text: &str) -> Result<FreqUnit, RepoError> {
    match text {
        "weekly" => Ok(FreqUnit::Weekly),
        "monthly" => Ok(FreqUnit::Monthly),
        "yearly" => Ok(FreqUnit::Yearly),
        other => Err(RepoError::Corrupt(format!(
            "invalid frequency unit {other:?}"
        ))),
    }
}

/// Splits a [`CategoryIcon`] into its `(kind, value)` columns.
pub(crate) fn icon_to_columns(icon: &CategoryIcon) -> (&'static str, &str) {
    match icon {
        CategoryIcon::Preset(value) => ("preset", value),
        CategoryIcon::Emoji(value) => ("emoji", value),
    }
}

/// Rebuilds a [`CategoryIcon`] from its `(kind, value)` columns.
pub(crate) fn icon_from_columns(kind: &str, value: String) -> Result<CategoryIcon, RepoError> {
    match kind {
        "preset" => Ok(CategoryIcon::Preset(value)),
        "emoji" => Ok(CategoryIcon::Emoji(value)),
        other => Err(RepoError::Corrupt(format!("invalid icon kind {other:?}"))),
    }
}

/// Splits a [`RuleEnd`] into its `(kind, Option<date text>)` columns.
pub(crate) fn rule_end_to_columns(end: RuleEnd) -> (&'static str, Option<String>) {
    match end {
        RuleEnd::Never => ("never", None),
        RuleEnd::Until(date) => ("until", Some(date_to_text(date))),
    }
}

/// Rebuilds a [`RuleEnd`] from its `(kind, Option<date text>)` columns.
pub(crate) fn rule_end_from_columns(
    kind: &str,
    date: Option<String>,
) -> Result<RuleEnd, RepoError> {
    match (kind, date) {
        ("never", None) => Ok(RuleEnd::Never),
        ("never", Some(_)) => Err(RepoError::Corrupt(
            "recurring rule end_kind='never' with an unexpected end_date".to_owned(),
        )),
        ("until", Some(text)) => Ok(RuleEnd::Until(date_from_text(&text)?)),
        ("until", None) => Err(RepoError::Corrupt(
            "recurring rule end_kind='until' with no end_date".to_owned(),
        )),
        (other, _) => Err(RepoError::Corrupt(format!(
            "invalid rule end kind {other:?}"
        ))),
    }
}
