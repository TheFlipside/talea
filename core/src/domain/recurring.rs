//! Recurring income/expense rules and their expansion into occurrences.
//!
//! A rule is a dateless entry template plus a start date, an end bound, and a
//! cadence. It is *expanded* into virtual entries for a queried month; the
//! occurrences are computed on demand, never stored, so editing a rule
//! re-derives everything.

use serde::{Deserialize, Serialize};
use time::{Date, Duration};

use crate::domain::date::clamp_day_in_month;
use crate::domain::entry::EntryKind;
use crate::domain::error::{validate_amount, DomainError, MAX_NOTE_LEN};
use crate::domain::ids::{AccountId, CategoryId, RecurringRuleId};
use crate::domain::month::Month;
use crate::money::Money;

/// The unit of a recurrence cadence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreqUnit {
    /// Every `interval` weeks.
    Weekly,
    /// Every `interval` months.
    Monthly,
    /// Every `interval` years.
    Yearly,
}

/// A cadence: a [`FreqUnit`] repeated every `interval` units (`interval >= 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "FrequencyRepr")]
pub struct Frequency {
    unit: FreqUnit,
    interval: u32,
}

#[derive(Deserialize)]
struct FrequencyRepr {
    unit: FreqUnit,
    interval: u32,
}

impl TryFrom<FrequencyRepr> for Frequency {
    type Error = DomainError;

    fn try_from(repr: FrequencyRepr) -> Result<Self, Self::Error> {
        Self::new(repr.unit, repr.interval)
    }
}

impl Frequency {
    /// Creates a cadence.
    ///
    /// # Errors
    ///
    /// [`DomainError::ZeroInterval`] if `interval` is zero.
    pub fn new(unit: FreqUnit, interval: u32) -> Result<Self, DomainError> {
        if interval == 0 {
            return Err(DomainError::ZeroInterval);
        }
        Ok(Self { unit, interval })
    }

    /// The cadence unit.
    #[must_use]
    pub const fn unit(self) -> FreqUnit {
        self.unit
    }

    /// The interval multiplier (`>= 1`).
    #[must_use]
    pub const fn interval(self) -> u32 {
        self.interval
    }
}

/// When a recurring rule stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RuleEndRepr", into = "RuleEndRepr")]
pub enum RuleEnd {
    /// The rule repeats indefinitely.
    Never,
    /// The rule repeats up to and including this date.
    Until(Date),
}

// Private (de)serialization mirror that keeps the `Until` date in our ISO
// `YYYY-MM-DD` string form, tagged for a stable JSON shape.
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RuleEndRepr {
    Never,
    Until {
        #[serde(with = "crate::domain::date::iso_date")]
        date: Date,
    },
}

impl From<RuleEnd> for RuleEndRepr {
    fn from(end: RuleEnd) -> Self {
        match end {
            RuleEnd::Never => Self::Never,
            RuleEnd::Until(date) => Self::Until { date },
        }
    }
}

impl From<RuleEndRepr> for RuleEnd {
    fn from(repr: RuleEndRepr) -> Self {
        match repr {
            RuleEndRepr::Never => Self::Never,
            RuleEndRepr::Until { date } => Self::Until(date),
        }
    }
}

/// An expanded occurrence of a [`RecurringRule`] within a month — shaped like an
/// [`Entry`](crate::domain::Entry) but without an id (it is not persisted).
///
/// Output-only: it is produced by [`RecurringRule::expand_in`] and serialized to
/// the frontend, but deliberately does **not** implement `Deserialize` — there
/// is no validating constructor for it, so accepting one over IPC would bypass
/// the invariants its source rule guarantees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VirtualEntry {
    account_id: AccountId,
    amount: Money,
    kind: EntryKind,
    #[serde(with = "crate::domain::date::iso_date")]
    date: Date,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category_id: Option<CategoryId>,
}

impl VirtualEntry {
    /// The account this occurrence belongs to.
    #[must_use]
    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }

    /// The positive magnitude.
    #[must_use]
    pub const fn amount(&self) -> Money {
        self.amount
    }

    /// Income or expense.
    #[must_use]
    pub const fn kind(&self) -> EntryKind {
        self.kind
    }

    /// The occurrence date.
    #[must_use]
    pub const fn date(&self) -> Date {
        self.date
    }

    /// Category classification, if any.
    #[must_use]
    pub const fn category_id(&self) -> Option<CategoryId> {
        self.category_id
    }

    /// The signed contribution to the balance.
    #[must_use]
    pub fn signed_amount(&self) -> Money {
        self.kind.signed(self.amount)
    }
}

/// A recurring income/expense rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RecurringRuleRepr")]
pub struct RecurringRule {
    id: RecurringRuleId,
    account_id: AccountId,
    amount: Money,
    kind: EntryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category_id: Option<CategoryId>,
    #[serde(with = "crate::domain::date::iso_date")]
    start_date: Date,
    end: RuleEnd,
    frequency: Frequency,
}

#[derive(Deserialize)]
struct RecurringRuleRepr {
    id: RecurringRuleId,
    account_id: AccountId,
    amount: Money,
    kind: EntryKind,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    category_id: Option<CategoryId>,
    #[serde(with = "crate::domain::date::iso_date")]
    start_date: Date,
    end: RuleEnd,
    frequency: Frequency,
}

impl TryFrom<RecurringRuleRepr> for RecurringRule {
    type Error = DomainError;

    fn try_from(repr: RecurringRuleRepr) -> Result<Self, Self::Error> {
        Self::new(
            repr.id,
            repr.account_id,
            repr.amount,
            repr.kind,
            repr.note,
            repr.category_id,
            repr.start_date,
            repr.end,
            repr.frequency,
        )
    }
}

impl RecurringRule {
    /// Creates a recurring rule.
    ///
    /// # Errors
    ///
    /// - [`DomainError::NonPositiveAmount`] / [`DomainError::AmountTooLarge`] if
    ///   `amount` is not a positive magnitude within the accepted ceiling.
    /// - [`DomainError::NoteTooLong`] if `note` exceeds [`MAX_NOTE_LEN`].
    /// - [`DomainError::EndBeforeStart`] if `end` is `Until(d)` with `d` before
    ///   `start_date`.
    #[allow(clippy::too_many_arguments)] // a rule genuinely has these fields; a builder is overkill here
    pub fn new(
        id: RecurringRuleId,
        account_id: AccountId,
        amount: Money,
        kind: EntryKind,
        note: Option<String>,
        category_id: Option<CategoryId>,
        start_date: Date,
        end: RuleEnd,
        frequency: Frequency,
    ) -> Result<Self, DomainError> {
        validate_amount(amount)?;
        if let Some(ref text) = note {
            let len = text.chars().count();
            if len > MAX_NOTE_LEN {
                return Err(DomainError::NoteTooLong {
                    len,
                    max: MAX_NOTE_LEN,
                });
            }
        }
        if let RuleEnd::Until(end_date) = end {
            if end_date < start_date {
                return Err(DomainError::EndBeforeStart {
                    start: start_date,
                    end: end_date,
                });
            }
        }
        Ok(Self {
            id,
            account_id,
            amount,
            kind,
            note,
            category_id,
            start_date,
            end,
            frequency,
        })
    }

    /// Stable identifier.
    #[must_use]
    pub const fn id(&self) -> RecurringRuleId {
        self.id
    }

    /// The account this rule belongs to.
    #[must_use]
    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }

    /// The cadence.
    #[must_use]
    pub const fn frequency(&self) -> Frequency {
        self.frequency
    }

    /// The positive magnitude applied on each occurrence.
    #[must_use]
    pub const fn amount(&self) -> Money {
        self.amount
    }

    /// Whether occurrences are income or expense.
    #[must_use]
    pub const fn kind(&self) -> EntryKind {
        self.kind
    }

    /// Free-text memo, if any.
    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// The category occurrences are classified under, if any.
    #[must_use]
    pub const fn category_id(&self) -> Option<CategoryId> {
        self.category_id
    }

    /// The cadence anchor (first possible occurrence).
    #[must_use]
    pub const fn start_date(&self) -> Date {
        self.start_date
    }

    /// When the rule stops recurring.
    #[must_use]
    pub const fn end(&self) -> RuleEnd {
        self.end
    }

    /// The dates on which this rule fires within `month`, ascending.
    ///
    /// Bounded by the month window; never iterates unboundedly. Empty if the
    /// rule has not started by the month's end or already ended before it.
    #[must_use]
    pub fn occurrences_in(&self, month: Month) -> Vec<Date> {
        let win_start = month.first_day();
        let win_end = month.last_day();

        if self.start_date > win_end {
            return Vec::new();
        }
        let hard_end = match self.end {
            RuleEnd::Never => win_end,
            RuleEnd::Until(end_date) => {
                if end_date < win_start {
                    return Vec::new();
                }
                end_date.min(win_end)
            }
        };

        match self.frequency.unit {
            FreqUnit::Weekly => self.weekly_occurrences(win_start, hard_end),
            FreqUnit::Monthly => self.monthly_occurrence(month, hard_end),
            FreqUnit::Yearly => self.yearly_occurrence(month, hard_end),
        }
    }

    /// Expands this rule into [`VirtualEntry`] occurrences within `month`.
    #[must_use]
    pub fn expand_in(&self, month: Month) -> Vec<VirtualEntry> {
        self.occurrences_in(month)
            .into_iter()
            .map(|date| VirtualEntry {
                account_id: self.account_id,
                amount: self.amount,
                kind: self.kind,
                date,
                note: self.note.clone(),
                category_id: self.category_id,
            })
            .collect()
    }

    fn weekly_occurrences(&self, win_start: Date, hard_end: Date) -> Vec<Date> {
        let step = i64::from(self.frequency.interval) * 7;
        // Closed-form first occurrence >= win_start, then step forward.
        let delta = (win_start - self.start_date).whole_days();
        let k0 = if delta <= 0 {
            0
        } else {
            (delta + step - 1) / step
        };

        let mut out = Vec::new();
        let Some(mut cur) = self.start_date.checked_add(Duration::days(k0 * step)) else {
            return out;
        };
        while cur <= hard_end {
            out.push(cur);
            cur = match cur.checked_add(Duration::days(step)) {
                Some(date) => date,
                None => break,
            };
        }
        out
    }

    fn monthly_occurrence(&self, month: Month, hard_end: Date) -> Vec<Date> {
        let start_month = Month::containing(self.start_date);
        let diff = month.index() - start_month.index();
        let interval = i64::from(self.frequency.interval);
        if diff < 0 || diff % interval != 0 {
            return Vec::new();
        }
        let occ = clamp_day_in_month(month.year(), month.as_time_month(), self.start_date.day());
        // `occ >= start_date` guards the start-month boundary; `<= hard_end`
        // applies the `Until` bound. Within this month the occurrence is unique.
        if occ >= self.start_date && occ <= hard_end {
            vec![occ]
        } else {
            Vec::new()
        }
    }

    fn yearly_occurrence(&self, month: Month, hard_end: Date) -> Vec<Date> {
        if month.month() != u8::from(self.start_date.month()) {
            return Vec::new();
        }
        let diff_years = i64::from(month.year()) - i64::from(self.start_date.year());
        let interval = i64::from(self.frequency.interval);
        if diff_years < 0 || diff_years % interval != 0 {
            return Vec::new();
        }
        let occ = clamp_day_in_month(month.year(), month.as_time_month(), self.start_date.day());
        // `occ >= start_date` guards the start-month boundary; `<= hard_end`
        // applies the `Until` bound. Within this month the occurrence is unique.
        if occ >= self.start_date && occ <= hard_end {
            vec![occ]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FreqUnit, Frequency, RecurringRule, RuleEnd};
    use crate::domain::entry::EntryKind;
    use crate::domain::error::DomainError;
    use crate::domain::ids::{AccountId, RecurringRuleId};
    use crate::domain::month::Month;
    use crate::money::Money;
    use time::{Date, Month as TMonth};

    fn date(y: i32, m: TMonth, d: u8) -> Date {
        Date::from_calendar_date(y, m, d).unwrap()
    }

    fn rule(start: Date, end: RuleEnd, freq: Frequency) -> RecurringRule {
        RecurringRule::new(
            RecurringRuleId::new(1),
            AccountId::new(1),
            Money::from_minor_units(1000, 2),
            EntryKind::Expense,
            None,
            None,
            start,
            end,
            freq,
        )
        .unwrap()
    }

    fn month(y: i32, m: u8) -> Month {
        Month::new(y, m).unwrap()
    }

    #[test]
    fn weekly_every_week() {
        let r = rule(
            date(2026, TMonth::January, 7),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Weekly, 1).unwrap(),
        );
        assert_eq!(
            r.occurrences_in(month(2026, 1)),
            vec![
                date(2026, TMonth::January, 7),
                date(2026, TMonth::January, 14),
                date(2026, TMonth::January, 21),
                date(2026, TMonth::January, 28),
            ]
        );
    }

    #[test]
    fn weekly_every_two_weeks_and_first_in_window() {
        let r = rule(
            date(2026, TMonth::January, 7),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Weekly, 2).unwrap(),
        );
        // Jan: 7, 21. Feb starts from the cadence continued: 4, 18.
        assert_eq!(
            r.occurrences_in(month(2026, 1)),
            vec![
                date(2026, TMonth::January, 7),
                date(2026, TMonth::January, 21)
            ]
        );
        assert_eq!(
            r.occurrences_in(month(2026, 2)),
            vec![
                date(2026, TMonth::February, 4),
                date(2026, TMonth::February, 18)
            ]
        );
    }

    #[test]
    fn weekly_before_start_is_empty() {
        let r = rule(
            date(2026, TMonth::March, 2),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Weekly, 1).unwrap(),
        );
        assert!(r.occurrences_in(month(2026, 1)).is_empty());
    }

    #[test]
    fn monthly_clamps_to_month_end_without_drift() {
        let r = rule(
            date(2026, TMonth::January, 31),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        );
        assert_eq!(
            r.occurrences_in(month(2026, 1)),
            vec![date(2026, TMonth::January, 31)]
        );
        // February clamps to 28 (2026 non-leap)...
        assert_eq!(
            r.occurrences_in(month(2026, 2)),
            vec![date(2026, TMonth::February, 28)]
        );
        // ...but March returns to the 31st (no drift from the Feb clamp).
        assert_eq!(
            r.occurrences_in(month(2026, 3)),
            vec![date(2026, TMonth::March, 31)]
        );
        assert_eq!(
            r.occurrences_in(month(2026, 4)),
            vec![date(2026, TMonth::April, 30)]
        );
    }

    #[test]
    fn monthly_leap_february() {
        let r = rule(
            date(2028, TMonth::January, 31),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        );
        assert_eq!(
            r.occurrences_in(month(2028, 2)),
            vec![date(2028, TMonth::February, 29)]
        );
    }

    #[test]
    fn monthly_every_two_months_gates_off_months() {
        let r = rule(
            date(2026, TMonth::January, 15),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 2).unwrap(),
        );
        assert_eq!(
            r.occurrences_in(month(2026, 1)),
            vec![date(2026, TMonth::January, 15)]
        );
        assert!(r.occurrences_in(month(2026, 2)).is_empty());
        assert_eq!(
            r.occurrences_in(month(2026, 3)),
            vec![date(2026, TMonth::March, 15)]
        );
    }

    #[test]
    fn yearly_clamps_feb_29() {
        let r = rule(
            date(2024, TMonth::February, 29),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Yearly, 1).unwrap(),
        );
        assert_eq!(
            r.occurrences_in(month(2024, 2)),
            vec![date(2024, TMonth::February, 29)]
        );
        assert_eq!(
            r.occurrences_in(month(2025, 2)),
            vec![date(2025, TMonth::February, 28)]
        );
        assert!(r.occurrences_in(month(2025, 3)).is_empty());
    }

    #[test]
    fn until_bound_is_inclusive() {
        let r = rule(
            date(2026, TMonth::January, 7),
            RuleEnd::Until(date(2026, TMonth::January, 21)),
            Frequency::new(FreqUnit::Weekly, 1).unwrap(),
        );
        assert_eq!(
            r.occurrences_in(month(2026, 1)),
            vec![
                date(2026, TMonth::January, 7),
                date(2026, TMonth::January, 14),
                date(2026, TMonth::January, 21),
            ]
        );
        assert!(r.occurrences_in(month(2026, 2)).is_empty());
    }

    #[test]
    fn zero_interval_rejected() {
        assert_eq!(
            Frequency::new(FreqUnit::Weekly, 0),
            Err(DomainError::ZeroInterval)
        );
    }

    #[test]
    fn end_before_start_rejected() {
        let err = RecurringRule::new(
            RecurringRuleId::new(1),
            AccountId::new(1),
            Money::from_minor_units(1000, 2),
            EntryKind::Expense,
            None,
            None,
            date(2026, TMonth::March, 1),
            RuleEnd::Until(date(2026, TMonth::February, 1)),
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        );
        assert!(matches!(err, Err(DomainError::EndBeforeStart { .. })));
    }

    #[test]
    fn rule_end_serde_shape() {
        let never = serde_json::to_string(&RuleEnd::Never).unwrap();
        assert_eq!(never, r#"{"kind":"never"}"#);
        let until =
            serde_json::to_string(&RuleEnd::Until(date(2026, TMonth::December, 31))).unwrap();
        assert_eq!(until, r#"{"kind":"until","date":"2026-12-31"}"#);
        assert_eq!(
            serde_json::from_str::<RuleEnd>(&until).unwrap(),
            RuleEnd::Until(date(2026, TMonth::December, 31))
        );
    }

    #[test]
    fn rule_round_trips() {
        let r = rule(
            date(2026, TMonth::January, 1),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        );
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains(r#""start_date":"2026-01-01""#));
        assert!(json.contains(r#""amount":"10.00""#));
        assert_eq!(serde_json::from_str::<RecurringRule>(&json).unwrap(), r);
    }
}
