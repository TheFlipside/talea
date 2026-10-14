//! Shared row↔domain conversion helpers.
//!
//! These translate between the database's TEXT/INTEGER columns and the domain's
//! types. The token strings here MUST match the `CHECK` constraints in the
//! migration and the domain's serde `rename_all = "snake_case"` output.

use talea_core::{CategoryIcon, EntryKind, FreqUnit, Money, Month, RuleEnd};
use time::Date;

use crate::error::RepoError;

/// Formats a date as ISO `YYYY-MM-DD` for storage.
pub(crate) fn date_to_text(date: Date) -> String {
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    date.format(fmt)
        .expect("formatting a valid date as ISO cannot fail")
}

/// At most the first 64 characters of `text`, to quote in an error. A stored
/// value can be arbitrarily long, and quoting one whole would copy all of it
/// (the convention `Money` and `Currency` keep for their own errors).
fn snippet(text: &str) -> String {
    text.chars().take(64).collect()
}

/// Parses an ISO `YYYY-MM-DD` date from storage, accepting only the exact text
/// [`date_to_text`] writes: `time` also reads other spellings, such as
/// `+2026-03-01`, which would let one date sit in a key under two.
pub(crate) fn date_from_text(text: &str) -> Result<Date, RepoError> {
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    Date::parse(text, fmt)
        .ok()
        .filter(|&date| date_to_text(date) == text)
        .ok_or_else(|| RepoError::Corrupt(format!("invalid date {:?}", snippet(text))))
}

/// Parses a stored money amount.
///
/// # Errors
/// [`RepoError::Corrupt`] if the text isn't a decimal amount.
pub(crate) fn money_from_text(text: &str) -> Result<Money, RepoError> {
    Money::try_from_str(text)
        .map_err(|_| RepoError::Corrupt(format!("invalid money {:?}", snippet(text))))
}

/// Splits a [`Month`] into its `(year, month)` `INTEGER` columns.
pub(crate) fn month_to_columns(month: Month) -> (i64, i64) {
    (i64::from(month.year()), i64::from(month.month()))
}

/// Rebuilds a [`Month`] from its `(year, month)` `INTEGER` columns.
///
/// # Errors
/// [`RepoError::Corrupt`] if the stored pair is out of range — the migration's
/// `CHECK`s make that unreachable short of external tampering.
pub(crate) fn month_from_columns(year: i64, month: i64) -> Result<Month, RepoError> {
    let corrupt = || RepoError::Corrupt(format!("invalid month {year}-{month}"));
    let year = i32::try_from(year).map_err(|_| corrupt())?;
    let month = u8::try_from(month).map_err(|_| corrupt())?;
    Month::new(year, month).map_err(|e| RepoError::corrupt(&e))
}

/// Rebuilds an optional [`Month`] from its nullable `(year, month)` columns.
///
/// # Errors
/// [`RepoError::Corrupt`] if exactly one of the pair is NULL (the migration's
/// paired-nullability `CHECK` forbids it) or the pair is out of range.
pub(crate) fn opt_month_from_columns(
    year: Option<i64>,
    month: Option<i64>,
) -> Result<Option<Month>, RepoError> {
    match (year, month) {
        (None, None) => Ok(None),
        (Some(year), Some(month)) => month_from_columns(year, month).map(Some),
        _ => Err(RepoError::Corrupt(
            "month columns must both be set or both be NULL".to_owned(),
        )),
    }
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
///
/// # Errors
/// [`RepoError::Corrupt`] if `rowid` isn't positive. AUTOINCREMENT hands out
/// ids from 1, so anything else was written from outside the app (a crafted
/// backup, say), and mapping it to a stand-in would alias distinct rows to one
/// id.
pub(crate) fn id_from_rowid(rowid: i64) -> Result<u64, RepoError> {
    u64::try_from(rowid)
        .ok()
        .filter(|&id| id > 0)
        .ok_or_else(|| RepoError::Corrupt(format!("invalid id {rowid}")))
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
        other => Err(RepoError::Corrupt(format!(
            "invalid entry kind {:?}",
            snippet(other)
        ))),
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
            "invalid frequency unit {:?}",
            snippet(other)
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
        other => Err(RepoError::Corrupt(format!(
            "invalid icon kind {:?}",
            snippet(other)
        ))),
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
            "invalid rule end kind {:?}",
            snippet(other)
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{date_from_text, date_to_text, money_from_text};
    use crate::error::RepoError;

    #[test]
    fn a_stored_date_reads_only_in_the_spelling_the_app_writes() {
        for text in [
            "0001-01-01",
            "2026-03-01",
            "9999-12-31",
            "0000-06-15",
            "-0001-01-01",
        ] {
            let date = date_from_text(text).unwrap();
            assert_eq!(date_to_text(date), text);
        }
        for text in [
            "+2026-03-01",
            "-0000-01-01",
            "2026-3-1",
            " 2026-03-01",
            "2026-03-01x",
        ] {
            assert!(date_from_text(text).is_err(), "{text:?}");
        }
    }

    /// The message of a corrupt-row error, or a panic if `err` is anything else.
    fn corrupt_message(err: Option<RepoError>) -> String {
        match err {
            Some(RepoError::Corrupt(message)) => message,
            other => panic!("expected a corrupt row, got {other:?}"),
        }
    }

    #[test]
    fn a_bad_stored_value_is_quoted_only_in_part() {
        let long = "x".repeat(10_000);
        for message in [
            corrupt_message(money_from_text(&long).err()),
            corrupt_message(date_from_text(&long).err()),
        ] {
            assert!(message.len() < 100, "{}", message.len());
        }
    }
}
