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
use crate::domain::error::{validate_amount, DomainError, MAX_AMOUNT_SEGMENTS, MAX_NOTE_LEN};
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

/// One step in a rule's amount history: `amount` applies to occurrences dated on
/// or after `effective_from`, until a later segment supersedes it.
///
/// A rule always has at least one segment — the base, anchored at its
/// `start_date`. Additional segments let an amount change *going forward* (e.g.
/// a raise) without rewriting past months, which is essential because the ledger
/// chains carry-over and a retroactive change would alter historical balances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "AmountSegmentRepr", into = "AmountSegmentRepr")]
pub struct AmountSegment {
    effective_from: Date,
    amount: Money,
}

// (De)serialization mirror keeping the date in our ISO `YYYY-MM-DD` string form.
#[derive(Serialize, Deserialize)]
struct AmountSegmentRepr {
    #[serde(with = "crate::domain::date::iso_date")]
    effective_from: Date,
    amount: Money,
}

impl From<AmountSegmentRepr> for AmountSegment {
    // Infallible: the per-segment amount and the cross-segment ordering are
    // validated when the owning `RecurringRule` is constructed.
    fn from(repr: AmountSegmentRepr) -> Self {
        Self {
            effective_from: repr.effective_from,
            amount: repr.amount,
        }
    }
}

impl From<AmountSegment> for AmountSegmentRepr {
    fn from(seg: AmountSegment) -> Self {
        Self {
            effective_from: seg.effective_from,
            amount: seg.amount,
        }
    }
}

impl AmountSegment {
    /// Creates a segment (validation of the amount happens at rule construction).
    #[must_use]
    pub const fn new(effective_from: Date, amount: Money) -> Self {
        Self {
            effective_from,
            amount,
        }
    }

    /// The date from which `amount` takes effect.
    #[must_use]
    pub const fn effective_from(&self) -> Date {
        self.effective_from
    }

    /// The positive magnitude applied from `effective_from` onward.
    #[must_use]
    pub const fn amount(&self) -> Money {
        self.amount
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
    rule_id: RecurringRuleId,
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
    /// The recurring rule that produced this occurrence.
    #[must_use]
    pub const fn rule_id(&self) -> RecurringRuleId {
        self.rule_id
    }

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
    /// Amount history, sorted ascending by `effective_from`; the first segment
    /// is the base, anchored at `start_date`. Never empty.
    amounts: Vec<AmountSegment>,
    kind: EntryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category_id: Option<CategoryId>,
    #[serde(with = "crate::domain::date::iso_date")]
    start_date: Date,
    end: RuleEnd,
    frequency: Frequency,
    /// Occurrence dates the user has individually removed ("skipped"), so the
    /// expansion omits them. Sorted ascending, unique. Internal only: skips are
    /// attached from storage after load and never cross the IPC boundary
    /// (`serde(skip)`), so an incoming rule never carries client-supplied skips.
    #[serde(skip)]
    skips: Vec<Date>,
}

#[derive(Deserialize)]
struct RecurringRuleRepr {
    id: RecurringRuleId,
    account_id: AccountId,
    amounts: Vec<AmountSegment>,
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
        Self::new_with_amounts(
            repr.id,
            repr.account_id,
            repr.amounts,
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
    /// Creates a recurring rule with a single (base) amount effective from
    /// `start_date`. Most rules are created this way; an amount that changes over
    /// time is built with [`RecurringRule::new_with_amounts`].
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
        Self::new_with_amounts(
            id,
            account_id,
            vec![AmountSegment::new(start_date, amount)],
            kind,
            note,
            category_id,
            start_date,
            end,
            frequency,
        )
    }

    /// Creates a recurring rule with an explicit amount history.
    ///
    /// `amounts` must be non-empty, strictly ascending by `effective_from`, with
    /// the first segment anchored exactly at `start_date`; every segment amount
    /// is validated like a single amount.
    ///
    /// # Errors
    ///
    /// - [`DomainError::InvalidAmountSegments`] if `amounts` is empty, not in
    ///   ascending date order, or its first segment is not at `start_date`.
    /// - [`DomainError::NonPositiveAmount`] / [`DomainError::AmountTooLarge`] for
    ///   any segment amount outside the accepted range.
    /// - [`DomainError::NoteTooLong`] / [`DomainError::EndBeforeStart`] as in
    ///   [`RecurringRule::new`].
    #[allow(clippy::too_many_arguments)] // a rule genuinely has these fields; a builder is overkill here
    pub fn new_with_amounts(
        id: RecurringRuleId,
        account_id: AccountId,
        amounts: Vec<AmountSegment>,
        kind: EntryKind,
        note: Option<String>,
        category_id: Option<CategoryId>,
        start_date: Date,
        end: RuleEnd,
        frequency: Frequency,
    ) -> Result<Self, DomainError> {
        let Some(first) = amounts.first() else {
            return Err(DomainError::InvalidAmountSegments);
        };
        if amounts.len() > MAX_AMOUNT_SEGMENTS {
            return Err(DomainError::TooManyAmountSegments {
                len: amounts.len(),
                max: MAX_AMOUNT_SEGMENTS,
            });
        }
        if first.effective_from != start_date {
            return Err(DomainError::InvalidAmountSegments);
        }
        // Strictly ascending effective dates (so each occurrence resolves to one
        // amount) and every amount within the accepted range.
        for pair in amounts.windows(2) {
            if pair[0].effective_from >= pair[1].effective_from {
                return Err(DomainError::InvalidAmountSegments);
            }
        }
        for segment in &amounts {
            validate_amount(segment.amount)?;
        }
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
            amounts,
            kind,
            note,
            category_id,
            start_date,
            end,
            frequency,
            skips: Vec::new(),
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

    /// The base amount, effective from `start_date`.
    #[must_use]
    pub fn base_amount(&self) -> Money {
        self.amounts[0].amount
    }

    /// The full amount history (ascending by `effective_from`, never empty).
    #[must_use]
    pub fn amounts(&self) -> &[AmountSegment] {
        &self.amounts
    }

    /// Returns the rule with the given occurrence dates marked as skipped
    /// (sorted, de-duplicated). Used by the persistence layer to attach stored
    /// per-occurrence removals; `expand_in` then omits these dates.
    #[must_use]
    pub fn with_skips(mut self, mut skips: Vec<Date>) -> Self {
        skips.sort_unstable();
        skips.dedup();
        self.skips = skips;
        self
    }

    /// The occurrence dates removed from this rule (sorted ascending).
    #[must_use]
    pub fn skips(&self) -> &[Date] {
        &self.skips
    }

    /// The amount in effect on `date`: the latest segment whose `effective_from`
    /// is on or before `date` (the base for any date at/after `start_date`).
    #[must_use]
    pub fn amount_on(&self, date: Date) -> Money {
        let mut amount = self.amounts[0].amount;
        for segment in &self.amounts {
            if segment.effective_from <= date {
                amount = segment.amount;
            } else {
                break;
            }
        }
        amount
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
            // Omit occurrences the user has individually removed (skipped).
            .filter(|date| self.skips.binary_search(date).is_err())
            .map(|date| VirtualEntry {
                rule_id: self.id,
                account_id: self.account_id,
                amount: self.amount_on(date),
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
    use super::{AmountSegment, FreqUnit, Frequency, RecurringRule, RuleEnd, VirtualEntry};
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
        assert!(json.contains(r#""amounts":[{"effective_from":"2026-01-01","amount":"10.00"}]"#));
        assert_eq!(serde_json::from_str::<RecurringRule>(&json).unwrap(), r);
    }

    fn rule_with_amounts(start: Date, amounts: Vec<AmountSegment>) -> RecurringRule {
        RecurringRule::new_with_amounts(
            RecurringRuleId::new(1),
            AccountId::new(1),
            amounts,
            EntryKind::Income,
            None,
            None,
            start,
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn amount_history_applies_per_occurrence_date() {
        // Base 1000 from Jan 15; raised to 1200 from Jun 1.
        let r = rule_with_amounts(
            date(2026, TMonth::January, 15),
            vec![
                AmountSegment::new(
                    date(2026, TMonth::January, 15),
                    Money::from_minor_units(100_000, 2),
                ),
                AmountSegment::new(
                    date(2026, TMonth::June, 1),
                    Money::from_minor_units(120_000, 2),
                ),
            ],
        );
        // Before the raise → base; on/after → raised.
        assert_eq!(
            r.amount_on(date(2026, TMonth::May, 15)),
            Money::from_minor_units(100_000, 2)
        );
        assert_eq!(
            r.amount_on(date(2026, TMonth::June, 15)),
            Money::from_minor_units(120_000, 2)
        );
        // The expansion picks the effective amount for the occurrence's date.
        let may = r.expand_in(month(2026, 5));
        assert_eq!(may[0].amount(), Money::from_minor_units(100_000, 2));
        let june = r.expand_in(month(2026, 6));
        assert_eq!(june[0].amount(), Money::from_minor_units(120_000, 2));
        assert_eq!(june[0].rule_id(), RecurringRuleId::new(1));
    }

    #[test]
    fn amount_segments_must_be_ordered_and_anchored() {
        let start = date(2026, TMonth::January, 1);
        let amt = Money::from_minor_units(1000, 2);
        // Empty.
        assert!(matches!(
            RecurringRule::new_with_amounts(
                RecurringRuleId::new(1),
                AccountId::new(1),
                vec![],
                EntryKind::Income,
                None,
                None,
                start,
                RuleEnd::Never,
                Frequency::new(FreqUnit::Monthly, 1).unwrap(),
            ),
            Err(DomainError::InvalidAmountSegments)
        ));
        // First segment not anchored at start_date.
        assert!(matches!(
            RecurringRule::new_with_amounts(
                RecurringRuleId::new(1),
                AccountId::new(1),
                vec![AmountSegment::new(date(2026, TMonth::February, 1), amt)],
                EntryKind::Income,
                None,
                None,
                start,
                RuleEnd::Never,
                Frequency::new(FreqUnit::Monthly, 1).unwrap(),
            ),
            Err(DomainError::InvalidAmountSegments)
        ));
        // Out of order / duplicate dates.
        assert!(matches!(
            RecurringRule::new_with_amounts(
                RecurringRuleId::new(1),
                AccountId::new(1),
                vec![
                    AmountSegment::new(start, amt),
                    AmountSegment::new(start, amt),
                ],
                EntryKind::Income,
                None,
                None,
                start,
                RuleEnd::Never,
                Frequency::new(FreqUnit::Monthly, 1).unwrap(),
            ),
            Err(DomainError::InvalidAmountSegments)
        ));
    }

    #[test]
    fn too_many_amount_segments_is_rejected() {
        let start = date(2026, TMonth::January, 1);
        let amt = Money::from_minor_units(1000, 2);
        // One base + MAX breakpoints exceeds the cap by one.
        let mut amounts = vec![AmountSegment::new(start, amt)];
        for i in 1..=super::MAX_AMOUNT_SEGMENTS {
            let offset = i64::try_from(i).unwrap();
            amounts.push(AmountSegment::new(
                start.saturating_add(time::Duration::days(offset)),
                amt,
            ));
        }
        assert!(matches!(
            RecurringRule::new_with_amounts(
                RecurringRuleId::new(1),
                AccountId::new(1),
                amounts,
                EntryKind::Income,
                None,
                None,
                start,
                RuleEnd::Never,
                Frequency::new(FreqUnit::Monthly, 1).unwrap(),
            ),
            Err(DomainError::TooManyAmountSegments { .. })
        ));
    }

    #[test]
    fn skipped_occurrences_are_omitted_from_expansion() {
        // Weekly rule in Jan 2026: 7, 14, 21, 28.
        let r = rule(
            date(2026, TMonth::January, 7),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Weekly, 1).unwrap(),
        )
        .with_skips(vec![date(2026, TMonth::January, 14)]);
        let dates: Vec<_> = r
            .expand_in(month(2026, 1))
            .iter()
            .map(VirtualEntry::date)
            .collect();
        assert_eq!(
            dates,
            vec![
                date(2026, TMonth::January, 7),
                date(2026, TMonth::January, 21),
                date(2026, TMonth::January, 28),
            ]
        );
        // The raw cadence (occurrences_in) is unaffected by skips.
        assert_eq!(r.occurrences_in(month(2026, 1)).len(), 4);
        // with_skips sorts/dedups.
        let messy = r.with_skips(vec![
            date(2026, TMonth::January, 28),
            date(2026, TMonth::January, 7),
            date(2026, TMonth::January, 28),
        ]);
        assert_eq!(
            messy.skips(),
            &[
                date(2026, TMonth::January, 7),
                date(2026, TMonth::January, 28)
            ]
        );
    }

    #[test]
    fn amount_history_round_trips_through_json() {
        let r = rule_with_amounts(
            date(2026, TMonth::January, 1),
            vec![
                AmountSegment::new(
                    date(2026, TMonth::January, 1),
                    Money::from_minor_units(1000, 2),
                ),
                AmountSegment::new(
                    date(2026, TMonth::July, 1),
                    Money::from_minor_units(1500, 2),
                ),
            ],
        );
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<RecurringRule>(&json).unwrap(), r);
    }
}
