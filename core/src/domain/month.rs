//! A budgeting period: a single calendar month.

use serde::{Deserialize, Serialize};
use time::Date;

use crate::domain::date::clamp_day_in_month;
use crate::domain::error::DomainError;

/// A calendar month. The unit the app's overview is built around.
///
/// Fields are private and validated: `month` is always `1..=12`. Ordering is
/// chronological (year dominates), which the ledger relies on — see the test
/// locking field order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "MonthRepr")]
pub struct Month {
    year: i32,
    month: u8,
}

/// Plain deserialization mirror; the `TryFrom` below routes it through
/// [`Month::new`] so an out-of-range month from JSON is rejected.
#[derive(Deserialize)]
struct MonthRepr {
    year: i32,
    month: u8,
}

impl TryFrom<MonthRepr> for Month {
    type Error = DomainError;

    fn try_from(repr: MonthRepr) -> Result<Self, Self::Error> {
        Self::new(repr.year, repr.month)
    }
}

impl Month {
    /// Creates a month.
    ///
    /// The year is bounded to `1..=9999` — the range `time::Date` supports — so
    /// date computations cannot panic and the carry-over chain length is
    /// bounded.
    ///
    /// # Errors
    ///
    /// [`DomainError::MonthOutOfRange`] if `month` is not in `1..=12`;
    /// [`DomainError::YearOutOfRange`] if `year` is not in `1..=9999`.
    pub fn new(year: i32, month: u8) -> Result<Self, DomainError> {
        if !(1..=9999).contains(&year) {
            return Err(DomainError::YearOutOfRange(year));
        }
        if !(1..=12).contains(&month) {
            return Err(DomainError::MonthOutOfRange(month));
        }
        Ok(Self { year, month })
    }

    /// The four-digit year.
    #[must_use]
    pub const fn year(self) -> i32 {
        self.year
    }

    /// The month of the year, `1..=12`.
    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    /// The following month (rolls into the next year after December).
    #[must_use]
    pub const fn succ(self) -> Self {
        if self.month == 12 {
            Self {
                year: self.year + 1,
                month: 1,
            }
        } else {
            Self {
                year: self.year,
                month: self.month + 1,
            }
        }
    }

    /// The preceding month (rolls into the previous year before January).
    #[must_use]
    pub const fn pred(self) -> Self {
        if self.month == 1 {
            Self {
                year: self.year - 1,
                month: 12,
            }
        } else {
            Self {
                year: self.year,
                month: self.month - 1,
            }
        }
    }

    /// The first day of this month.
    #[must_use]
    pub fn first_day(self) -> Date {
        clamp_day_in_month(self.year, self.as_time_month(), 1)
    }

    /// The last day of this month (28/29/30/31 as appropriate).
    #[must_use]
    pub fn last_day(self) -> Date {
        clamp_day_in_month(self.year, self.as_time_month(), 31)
    }

    /// The month that `date` falls in.
    #[must_use]
    pub fn containing(date: Date) -> Self {
        Self {
            year: date.year(),
            month: u8::from(date.month()),
        }
    }

    /// Whether `date` falls within this month.
    #[must_use]
    pub fn contains(self, date: Date) -> bool {
        date.year() == self.year && u8::from(date.month()) == self.month
    }

    /// This month as a `time::Month`.
    pub(crate) fn as_time_month(self) -> time::Month {
        time::Month::try_from(self.month).expect("month is validated to 1..=12")
    }

    /// A monotonically increasing absolute month index, for cadence arithmetic.
    pub(crate) fn index(self) -> i64 {
        i64::from(self.year) * 12 + i64::from(self.month) - 1
    }
}

#[cfg(test)]
mod tests {
    use super::Month;
    use crate::domain::error::DomainError;
    use time::{Date, Month as TMonth};

    #[test]
    fn rejects_out_of_range_month() {
        assert_eq!(Month::new(2026, 0), Err(DomainError::MonthOutOfRange(0)));
        assert_eq!(Month::new(2026, 13), Err(DomainError::MonthOutOfRange(13)));
        assert!(Month::new(2026, 1).is_ok());
        assert!(Month::new(2026, 12).is_ok());
    }

    #[test]
    fn rejects_out_of_range_year() {
        assert_eq!(Month::new(0, 1), Err(DomainError::YearOutOfRange(0)));
        assert_eq!(
            Month::new(10_000, 1),
            Err(DomainError::YearOutOfRange(10_000))
        );
        assert_eq!(Month::new(-5, 1), Err(DomainError::YearOutOfRange(-5)));
        assert!(Month::new(1, 1).is_ok());
        assert!(Month::new(9999, 12).is_ok());
    }

    #[test]
    fn ordering_is_chronological() {
        // Locks the field order (year before month). Dec 2025 precedes Jan 2026.
        assert!(Month::new(2025, 12).unwrap() < Month::new(2026, 1).unwrap());
        assert!(Month::new(2026, 1).unwrap() > Month::new(2025, 12).unwrap());
    }

    #[test]
    fn succ_and_pred_roll_the_year() {
        assert_eq!(
            Month::new(2026, 12).unwrap().succ(),
            Month::new(2027, 1).unwrap()
        );
        assert_eq!(
            Month::new(2026, 1).unwrap().pred(),
            Month::new(2025, 12).unwrap()
        );
    }

    #[test]
    fn first_and_last_day() {
        let feb = Month::new(2025, 2).unwrap();
        assert_eq!(
            feb.first_day(),
            Date::from_calendar_date(2025, TMonth::February, 1).unwrap()
        );
        assert_eq!(
            feb.last_day(),
            Date::from_calendar_date(2025, TMonth::February, 28).unwrap()
        );
        let feb_leap = Month::new(2024, 2).unwrap();
        assert_eq!(
            feb_leap.last_day(),
            Date::from_calendar_date(2024, TMonth::February, 29).unwrap()
        );
    }

    #[test]
    fn containing_and_contains() {
        let d = Date::from_calendar_date(2026, TMonth::March, 15).unwrap();
        assert_eq!(Month::containing(d), Month::new(2026, 3).unwrap());
        assert!(Month::new(2026, 3).unwrap().contains(d));
        assert!(!Month::new(2026, 4).unwrap().contains(d));
    }

    #[test]
    fn deserialize_rejects_invalid_month() {
        assert!(serde_json::from_str::<Month>(r#"{"year":2026,"month":13}"#).is_err());
        let m: Month = serde_json::from_str(r#"{"year":2026,"month":5}"#).unwrap();
        assert_eq!(m, Month::new(2026, 5).unwrap());
    }
}
