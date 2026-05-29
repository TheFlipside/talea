//! Calendar-date helpers and the JSON boundary format for dates.
//!
//! Dates cross the frontend/IPC boundary as ISO-8601 `YYYY-MM-DD` **strings**,
//! controlled here rather than relying on `time`'s default `Date` serde — the
//! same discipline applied to [`Money`](crate::money::Money).

use time::Date;

// Generates a module usable with `#[serde(with = "...::iso_date")]`, plus a
// nested `iso_date::option` for `Option<Date>` fields. `pub(crate)` so the
// other domain modules can reference it via `#[serde(with = ...)]`.
time::serde::format_description!(pub(crate) iso_date, Date, "[year]-[month]-[day]");

/// Day `day` of (`year`, `month`), **clamped down** to the month's last valid
/// day (and up to day 1). So `(2025, Feb, 31) -> 2025-02-28`,
/// `(2024, Feb, 31) -> 2024-02-29`, `(2026, Apr, 31) -> 2026-04-30`.
///
/// Clamping always derives from the caller's intended `day`, never from a
/// previously clamped result, so a "31st" rule does not drift to the 28th after
/// passing through February.
pub(crate) fn clamp_day_in_month(year: i32, month: time::Month, day: u8) -> Date {
    let last = month.length(year);
    let clamped = day.clamp(1, last);
    Date::from_calendar_date(year, month, clamped).expect("clamped day is always valid")
}

#[cfg(test)]
mod tests {
    use super::{clamp_day_in_month, iso_date, Date};
    use serde::{Deserialize, Serialize};
    use time::Month;

    #[test]
    fn clamps_to_month_length() {
        assert_eq!(
            clamp_day_in_month(2025, Month::February, 31),
            Date::from_calendar_date(2025, Month::February, 28).unwrap()
        );
        assert_eq!(
            clamp_day_in_month(2024, Month::February, 31),
            Date::from_calendar_date(2024, Month::February, 29).unwrap()
        );
        assert_eq!(
            clamp_day_in_month(2026, Month::April, 31),
            Date::from_calendar_date(2026, Month::April, 30).unwrap()
        );
        // No clamping needed.
        assert_eq!(
            clamp_day_in_month(2026, Month::January, 31),
            Date::from_calendar_date(2026, Month::January, 31).unwrap()
        );
    }

    #[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
    struct Wrapper {
        #[serde(with = "iso_date")]
        date: Date,
        #[serde(with = "iso_date::option")]
        maybe: Option<Date>,
    }

    #[test]
    fn serializes_as_iso_string() {
        let w = Wrapper {
            date: Date::from_calendar_date(2026, Month::February, 9).unwrap(),
            maybe: None,
        };
        let json = serde_json::to_string(&w).unwrap();
        assert_eq!(json, r#"{"date":"2026-02-09","maybe":null}"#);
        assert_eq!(serde_json::from_str::<Wrapper>(&json).unwrap(), w);
    }

    #[test]
    fn rejects_non_iso_date() {
        assert!(serde_json::from_str::<Wrapper>(r#"{"date":"2026/02/09","maybe":null}"#).is_err());
        // A bare number is not a date string.
        assert!(serde_json::from_str::<Wrapper>(r#"{"date":20260209,"maybe":null}"#).is_err());
    }
}
