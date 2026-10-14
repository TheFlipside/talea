//! Category budgets: a planned monthly amount for one category, consumed by
//! that month's real spending in the same category.
//!
//! A recurring rule models a cost that leaves the account in one lump. A budget
//! models the other shape: a planned amount that gets spent across the month in
//! many purchases which *ideally* sum to the plan. The budget **holds back** its
//! amount, and real spending **consumes** the reservation instead of stacking on
//! top of it:
//!
//! ```text
//! reserved = Σ over budgets of max(0, planned − actual)
//! free     = available − reserved
//! ```
//!
//! This is a **reservation overlay on the cashflow ledger, not envelope
//! budgeting**: a budget never moves money, never funds anything, and never
//! touches the carry-over chain. `available` keeps its meaning exactly
//! (`carry_in + income − expenses`), so unspent budget is simply ordinary
//! surplus that carries into the next month like any other money — there is no
//! rollover into next month's budget. See `docs/DESIGN.md` §12.

use serde::{Deserialize, Serialize};

use crate::domain::entry::Entry;
use crate::domain::error::{validate_amount, DomainError, MAX_AMOUNT_SEGMENTS};
use crate::domain::ids::{AccountId, BudgetId, CategoryId};
use crate::domain::month::Month;
use crate::domain::recurring::RecurringRule;
use crate::domain::stats::expenses_by_category;
use crate::money::Money;

/// One step in a budget's amount history: `amount` applies from
/// `effective_from` until a later segment supersedes it.
///
/// Month-keyed rather than date-keyed (unlike a rule's
/// [`AmountSegment`](crate::domain::AmountSegment)) because a budget is defined
/// per month throughout — `start` and `end` are months too. Validation of the
/// ordering lives in [`Budget::new_with_amounts`], which owns the sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetSegment {
    effective_from: Month,
    amount: Money,
}

impl BudgetSegment {
    /// Creates a segment. Unvalidated on its own — the owning [`Budget`] checks
    /// the amount and the sequence's anchoring and ordering.
    #[must_use]
    pub const fn new(effective_from: Month, amount: Money) -> Self {
        Self {
            effective_from,
            amount,
        }
    }

    /// The first month this amount applies to.
    #[must_use]
    pub const fn effective_from(&self) -> Month {
        self.effective_from
    }

    /// The planned amount from [`effective_from`](BudgetSegment::effective_from).
    #[must_use]
    pub const fn amount(&self) -> Money {
        self.amount
    }
}

/// A planned monthly amount for one category on one account.
///
/// Applies from [`start`](Budget::start) onwards, up to and including
/// [`end`](Budget::end) when one is set. "At most one budget per category per
/// account" is a cross-row rule this crate cannot see; it is enforced by the
/// shell (schema constraint plus a command-layer check), exactly as summary
/// membership's currency rule is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "BudgetRepr")]
pub struct Budget {
    id: BudgetId,
    account_id: AccountId,
    category_id: CategoryId,
    /// Amount history, sorted ascending by `effective_from`; the first segment
    /// is the base, anchored at `start`. Never empty.
    amounts: Vec<BudgetSegment>,
    start: Month,
    /// `None` means open-ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    end: Option<Month>,
}

/// Plain deserialization mirror; the `TryFrom` below routes it through
/// [`Budget::new_with_amounts`] so a malformed budget from JSON is rejected.
#[derive(Deserialize)]
struct BudgetRepr {
    id: BudgetId,
    account_id: AccountId,
    category_id: CategoryId,
    amounts: Vec<BudgetSegment>,
    start: Month,
    #[serde(default)]
    end: Option<Month>,
}

impl TryFrom<BudgetRepr> for Budget {
    type Error = DomainError;

    fn try_from(repr: BudgetRepr) -> Result<Self, Self::Error> {
        Self::new_with_amounts(
            repr.id,
            repr.account_id,
            repr.category_id,
            repr.amounts,
            repr.start,
            repr.end,
        )
    }
}

impl Budget {
    /// Creates a category budget with a single amount, effective from `start`.
    ///
    /// # Errors
    ///
    /// [`DomainError::NonPositiveAmount`] / [`DomainError::AmountTooLarge`] if
    /// `amount` is not a positive magnitude within the accepted ceiling;
    /// [`DomainError::BudgetEndBeforeStart`] if `end` precedes `start`.
    pub fn new(
        id: BudgetId,
        account_id: AccountId,
        category_id: CategoryId,
        amount: Money,
        start: Month,
        end: Option<Month>,
    ) -> Result<Self, DomainError> {
        Self::new_with_amounts(
            id,
            account_id,
            category_id,
            vec![BudgetSegment::new(start, amount)],
            start,
            end,
        )
    }

    /// Creates a category budget with a full amount history.
    ///
    /// `amounts` must be non-empty, at most [`MAX_AMOUNT_SEGMENTS`] long,
    /// anchored at `start`, and strictly ascending — so every month resolves to
    /// exactly one amount.
    ///
    /// # Errors
    ///
    /// [`DomainError::InvalidAmountSegments`] if the sequence is empty, not
    /// anchored at `start`, or not strictly ascending;
    /// [`DomainError::TooManyAmountSegments`] if it exceeds the cap;
    /// [`DomainError::NonPositiveAmount`] / [`DomainError::AmountTooLarge`] for a
    /// bad amount; [`DomainError::BudgetEndBeforeStart`] if `end` precedes
    /// `start`.
    pub fn new_with_amounts(
        id: BudgetId,
        account_id: AccountId,
        category_id: CategoryId,
        amounts: Vec<BudgetSegment>,
        start: Month,
        end: Option<Month>,
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
        if first.effective_from != start {
            return Err(DomainError::InvalidAmountSegments);
        }
        // Strictly ascending, so `amount_in` resolves each month unambiguously.
        for pair in amounts.windows(2) {
            if pair[0].effective_from >= pair[1].effective_from {
                return Err(DomainError::InvalidAmountSegments);
            }
        }
        for segment in &amounts {
            validate_amount(segment.amount)?;
        }
        if let Some(end) = end {
            if end < start {
                return Err(DomainError::BudgetEndBeforeStart { start, end });
            }
        }
        Ok(Self {
            id,
            account_id,
            category_id,
            amounts,
            start,
            end,
        })
    }

    /// The budget's identifier.
    #[must_use]
    pub const fn id(&self) -> BudgetId {
        self.id
    }

    /// The account this budget belongs to.
    #[must_use]
    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }

    /// The category whose spending this budget measures.
    #[must_use]
    pub const fn category_id(&self) -> CategoryId {
        self.category_id
    }

    /// The base planned amount, effective from [`start`](Budget::start).
    ///
    /// Prefer [`Budget::amount_in`] when computing a month's figures — see the
    /// note there.
    #[must_use]
    pub fn base_amount(&self) -> Money {
        self.amounts[0].amount
    }

    /// The full amount history, ascending by month. Never empty.
    #[must_use]
    pub fn amounts(&self) -> &[BudgetSegment] {
        &self.amounts
    }

    /// The first month this budget applies to.
    #[must_use]
    pub const fn start(&self) -> Month {
        self.start
    }

    /// The last month this budget applies to, or `None` if open-ended.
    #[must_use]
    pub const fn end(&self) -> Option<Month> {
        self.end
    }

    /// The planned amount in effect for `month`: the last segment effective at
    /// or before it.
    ///
    /// Months before `start` cannot occur in practice (`applies_in` gates them),
    /// but they resolve to the base amount rather than panicking.
    #[must_use]
    pub fn amount_in(&self, month: Month) -> Money {
        let mut current = self.amounts[0].amount;
        for segment in &self.amounts {
            if segment.effective_from > month {
                break;
            }
            current = segment.amount;
        }
        current
    }

    /// Whether this budget reserves anything in `month`.
    #[must_use]
    pub fn applies_in(&self, month: Month) -> bool {
        month >= self.start && self.end.is_none_or(|end| month <= end)
    }
}

/// One budget's progress within a month.
///
/// Output-only: like [`MonthSummary`](crate::domain::MonthSummary) and
/// [`CategoryExpense`](crate::domain::CategoryExpense) it deliberately does
/// **not** implement `Deserialize` — every figure is derived from the raw
/// budgets/entries/rules, never accepted from outside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BudgetStatus {
    /// The budget described.
    pub budget_id: BudgetId,
    /// The account the budget belongs to. A summary account's list concatenates
    /// its members' rows, so the shell can tag each by its source account.
    pub account_id: AccountId,
    /// The category whose spending is measured.
    pub category_id: CategoryId,
    /// The planned amount for this month (positive).
    pub planned: Money,
    /// Actually spent in the category this month (positive).
    pub actual: Money,
    /// Still held back: `max(0, planned − actual)`. Zero once the budget is
    /// spent out or overspent.
    pub reserved: Money,
}

/// Each budget applicable in `month`, with its progress, in the given order.
///
/// `actual` reuses [`expenses_by_category`], so a budget measures exactly what
/// the statistics screen reports for that category — including recurring-rule
/// expansions and honouring per-occurrence skips — in one pass shared by every
/// budget rather than one walk per budget.
#[must_use]
pub fn budget_status(
    month: Month,
    budgets: &[Budget],
    entries: &[Entry],
    rules: &[RecurringRule],
) -> Vec<BudgetStatus> {
    // Nothing applies, so there is nothing to measure and no reason to walk the
    // ledger: `summaries_for_range` calls this once per month over a range that
    // may span a century.
    if !budgets.iter().any(|budget| budget.applies_in(month)) {
        return Vec::new();
    }

    let spent = expenses_by_category(month, entries, rules);

    budgets
        .iter()
        .filter(|budget| budget.applies_in(month))
        .map(|budget| {
            let planned = budget.amount_in(month);
            let actual = spent
                .iter()
                .find(|row| row.category_id == Some(budget.category_id()))
                .map_or_else(Money::zero, |row| row.total);
            // `Money`'s `-` panics on overflow by design (see DESIGN.md §9).
            // `planned` is bounded by the per-amount cap; `actual` is a category
            // total, so it is bounded only the way every other ledger sum is —
            // that cap times a realistic entry count, far below `Decimal`'s range.
            let remaining = planned - actual;
            BudgetStatus {
                budget_id: budget.id(),
                account_id: budget.account_id(),
                category_id: budget.category_id(),
                planned,
                actual,
                reserved: if remaining.is_negative() {
                    Money::zero()
                } else {
                    remaining
                },
            }
        })
        .collect()
}

/// Total held back by `budgets` in `month`: `Σ max(0, planned − actual)`.
///
/// This is what [`month_summary`](crate::domain::month_summary) subtracts from
/// `available` to get `free`. It deliberately leaves `available` alone —
/// unspent budget stays ordinary surplus and carries into the next month like
/// any other money.
#[must_use]
pub fn reserved_in(
    month: Month,
    budgets: &[Budget],
    entries: &[Entry],
    rules: &[RecurringRule],
) -> Money {
    budget_status(month, budgets, entries, rules)
        .iter()
        .fold(Money::zero(), |total, status| total + status.reserved)
}

#[cfg(test)]
mod tests {
    use super::{budget_status, reserved_in, Budget, BudgetSegment};
    use crate::domain::entry::{Entry, EntryKind};
    use crate::domain::error::{DomainError, MAX_AMOUNT_SEGMENTS};
    use crate::domain::ids::{AccountId, BudgetId, CategoryId, EntryId, RecurringRuleId};
    use crate::domain::month::Month;
    use crate::domain::recurring::{FreqUnit, Frequency, RecurringRule, RuleEnd};
    use crate::money::Money;
    use time::{Date, Month as TMonth};

    fn m(y: i32, mo: u8) -> Month {
        Month::new(y, mo).unwrap()
    }

    fn money(minor: i64) -> Money {
        Money::from_minor_units(minor, 2)
    }

    /// A budget of `minor` on category 1, running from Jan 2026 to `end`.
    fn budget(minor: i64, end: Option<Month>) -> Budget {
        Budget::new(
            BudgetId::new(1),
            AccountId::new(1),
            CategoryId::new(1),
            money(minor),
            m(2026, 1),
            end,
        )
        .unwrap()
    }

    fn entry(id: u64, minor: i64, kind: EntryKind, day: u8, category: Option<u64>) -> Entry {
        Entry::new(
            EntryId::new(id),
            AccountId::new(1),
            money(minor),
            kind,
            Date::from_calendar_date(2026, TMonth::January, day).unwrap(),
            None,
            category.map(CategoryId::new),
        )
        .unwrap()
    }

    /// A monthly expense rule of `minor` on category 1, from Jan 2026.
    fn rule(minor: i64) -> RecurringRule {
        RecurringRule::new(
            RecurringRuleId::new(1),
            AccountId::new(1),
            money(minor),
            EntryKind::Expense,
            None,
            Some(CategoryId::new(1)),
            Date::from_calendar_date(2026, TMonth::January, 1).unwrap(),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn reserves_the_unspent_part_of_a_budget() {
        let budgets = [budget(40_000, None)];
        let out = budget_status(m(2026, 1), &budgets, &[], &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].planned, money(40_000));
        assert_eq!(out[0].actual, Money::zero());
        assert_eq!(out[0].reserved, money(40_000));
    }

    #[test]
    fn spending_reduces_the_reservation_one_for_one() {
        let budgets = [budget(40_000, None)];
        let entries = [entry(1, 15_000, EntryKind::Expense, 5, Some(1))];
        let out = budget_status(m(2026, 1), &budgets, &entries, &[]);
        assert_eq!(out[0].actual, money(15_000));
        // 400 planned − 150 spent → 250 still held back.
        assert_eq!(out[0].reserved, money(25_000));
    }

    #[test]
    fn overspending_a_budget_reserves_nothing() {
        let budgets = [budget(40_000, None)];
        let entries = [entry(1, 50_000, EntryKind::Expense, 5, Some(1))];
        let out = budget_status(m(2026, 1), &budgets, &entries, &[]);
        assert_eq!(out[0].actual, money(50_000));
        // Clamped at zero: the overspend is already in `expenses`, so counting it
        // again here would subtract it from `available` twice.
        assert_eq!(out[0].reserved, Money::zero());
    }

    #[test]
    fn ignores_expenses_in_other_categories() {
        let budgets = [budget(40_000, None)];
        let entries = [
            entry(1, 15_000, EntryKind::Expense, 5, Some(2)),
            entry(2, 9_000, EntryKind::Expense, 6, None),
        ];
        let out = budget_status(m(2026, 1), &budgets, &entries, &[]);
        assert_eq!(out[0].actual, Money::zero());
        assert_eq!(out[0].reserved, money(40_000));
    }

    #[test]
    fn income_in_the_category_is_not_actual() {
        let budgets = [budget(40_000, None)];
        let entries = [entry(1, 15_000, EntryKind::Income, 5, Some(1))];
        let out = budget_status(m(2026, 1), &budgets, &entries, &[]);
        assert_eq!(out[0].actual, Money::zero());
    }

    #[test]
    fn counts_recurring_expansions_as_actual() {
        // A rule in a budgeted category books against the budget just like an ad
        // hoc entry does — it is real planned spending, and it is what the stats
        // screen reports for that category.
        let budgets = [budget(40_000, None)];
        let rules = [rule(10_000)];
        let entries = [entry(1, 5_000, EntryKind::Expense, 5, Some(1))];
        let out = budget_status(m(2026, 1), &budgets, &entries, &rules);
        assert_eq!(out[0].actual, money(15_000));
        assert_eq!(out[0].reserved, money(25_000));
    }

    #[test]
    fn applies_only_from_its_start_month() {
        let budgets = [budget(40_000, None)];
        assert_eq!(budget_status(m(2025, 12), &budgets, &[], &[]), []);
        assert_eq!(budget_status(m(2026, 1), &budgets, &[], &[]).len(), 1);
    }

    #[test]
    fn applies_up_to_and_including_its_end_month() {
        let budgets = [budget(40_000, Some(m(2026, 3)))];
        assert_eq!(budget_status(m(2026, 3), &budgets, &[], &[]).len(), 1);
        assert_eq!(budget_status(m(2026, 4), &budgets, &[], &[]), []);
    }

    #[test]
    fn an_open_ended_budget_never_stops_applying() {
        let budgets = [budget(40_000, None)];
        assert_eq!(budget_status(m(9999, 12), &budgets, &[], &[]).len(), 1);
    }

    #[test]
    fn status_order_follows_the_budget_list() {
        let second = Budget::new(
            BudgetId::new(2),
            AccountId::new(1),
            CategoryId::new(2),
            money(10_000),
            m(2026, 1),
            None,
        )
        .unwrap();
        let budgets = [budget(40_000, None), second];
        let out = budget_status(m(2026, 1), &budgets, &[], &[]);
        assert_eq!(out[0].budget_id, BudgetId::new(1));
        assert_eq!(out[1].budget_id, BudgetId::new(2));
    }

    #[test]
    fn reserved_in_sums_every_applicable_budget() {
        let second = Budget::new(
            BudgetId::new(2),
            AccountId::new(1),
            CategoryId::new(2),
            money(10_000),
            m(2026, 1),
            None,
        )
        .unwrap();
        let budgets = [budget(40_000, None), second];
        let entries = [entry(1, 15_000, EntryKind::Expense, 5, Some(1))];
        // (400 − 150) + (100 − 0) = 350.
        assert_eq!(
            reserved_in(m(2026, 1), &budgets, &entries, &[]),
            money(35_000)
        );
    }

    #[test]
    fn reserved_in_is_zero_without_budgets() {
        let entries = [entry(1, 15_000, EntryKind::Expense, 5, Some(1))];
        assert_eq!(reserved_in(m(2026, 1), &[], &entries, &[]), Money::zero());
    }

    #[test]
    fn rejects_a_non_positive_amount() {
        let err = Budget::new(
            BudgetId::new(1),
            AccountId::new(1),
            CategoryId::new(1),
            Money::zero(),
            m(2026, 1),
            None,
        );
        assert_eq!(err, Err(DomainError::NonPositiveAmount(Money::zero())));
    }

    #[test]
    fn rejects_an_end_before_its_start() {
        let err = Budget::new(
            BudgetId::new(1),
            AccountId::new(1),
            CategoryId::new(1),
            money(40_000),
            m(2026, 5),
            Some(m(2026, 4)),
        );
        assert_eq!(
            err,
            Err(DomainError::BudgetEndBeforeStart {
                start: m(2026, 5),
                end: m(2026, 4),
            })
        );
        // The same month is fine — a one-month budget.
        assert!(Budget::new(
            BudgetId::new(1),
            AccountId::new(1),
            CategoryId::new(1),
            money(40_000),
            m(2026, 5),
            Some(m(2026, 5)),
        )
        .is_ok());
    }

    #[test]
    fn round_trips_as_json() {
        let original = budget(40_000, None);
        let json = serde_json::to_string(&original).unwrap();
        assert!(
            json.contains(
                r#""amounts":[{"effective_from":{"year":2026,"month":1},"amount":"400.00"}]"#
            ),
            "{json}"
        );
        assert!(
            json.contains(r#""start":{"year":2026,"month":1}"#),
            "{json}"
        );
        // Open-ended budgets omit `end` entirely rather than sending null.
        assert!(!json.contains(r#""end""#), "{json}");
        assert_eq!(serde_json::from_str::<Budget>(&json).unwrap(), original);

        let bounded = budget(40_000, Some(m(2026, 6)));
        let json = serde_json::to_string(&bounded).unwrap();
        assert!(json.contains(r#""end":{"year":2026,"month":6}"#), "{json}");
        assert_eq!(serde_json::from_str::<Budget>(&json).unwrap(), bounded);
    }

    #[test]
    fn rejects_a_tampered_amount_at_deserialize() {
        // Invariants unreachable through the constructor still have to hold for
        // JSON arriving over IPC or out of storage.
        let json = r#"{"id":1,"account_id":1,"category_id":1,
                       "amounts":[{"effective_from":{"year":2026,"month":1},"amount":"-5.00"}],
                       "start":{"year":2026,"month":1}}"#;
        assert!(serde_json::from_str::<Budget>(json).is_err());
    }

    #[test]
    fn rejects_numeric_json_to_keep_floats_out() {
        let json = r#"{"id":1,"account_id":1,"category_id":1,
                       "amounts":[{"effective_from":{"year":2026,"month":1},"amount":400.0}],
                       "start":{"year":2026,"month":1}}"#;
        assert!(serde_json::from_str::<Budget>(json).is_err());
    }

    // ---- Amount history ----------------------------------------------------

    /// A budget from Jan 2026 whose amount changes at the given breakpoints.
    fn budget_with(segments: &[(Month, i64)]) -> Budget {
        Budget::new_with_amounts(
            BudgetId::new(1),
            AccountId::new(1),
            CategoryId::new(1),
            segments
                .iter()
                .map(|(month, minor)| BudgetSegment::new(*month, money(*minor)))
                .collect(),
            m(2026, 1),
            None,
        )
        .unwrap()
    }

    #[test]
    fn a_single_amount_applies_to_every_month() {
        let b = budget(40_000, None);
        assert_eq!(b.amount_in(m(2026, 1)), money(40_000));
        assert_eq!(b.amount_in(m(2030, 7)), money(40_000));
        assert_eq!(b.base_amount(), money(40_000));
        assert_eq!(b.amounts().len(), 1);
    }

    #[test]
    fn a_breakpoint_applies_from_its_month_onward() {
        // 400 from January, raised to 450 from June.
        let b = budget_with(&[(m(2026, 1), 40_000), (m(2026, 6), 45_000)]);
        assert_eq!(b.amount_in(m(2026, 5)), money(40_000));
        assert_eq!(b.amount_in(m(2026, 6)), money(45_000));
        assert_eq!(b.amount_in(m(2027, 1)), money(45_000));
        // The base is untouched, which is the whole point: past months keep the
        // figure that was actually planned at the time.
        assert_eq!(b.base_amount(), money(40_000));
    }

    #[test]
    fn the_latest_applicable_breakpoint_wins() {
        let b = budget_with(&[
            (m(2026, 1), 40_000),
            (m(2026, 6), 45_000),
            (m(2026, 9), 50_000),
        ]);
        assert_eq!(b.amount_in(m(2026, 8)), money(45_000));
        assert_eq!(b.amount_in(m(2026, 9)), money(50_000));
    }

    #[test]
    fn reserving_uses_the_months_own_amount() {
        let budgets = [budget_with(&[(m(2026, 1), 40_000), (m(2026, 6), 45_000)])];
        assert_eq!(
            budget_status(m(2026, 1), &budgets, &[], &[])[0].planned,
            money(40_000)
        );
        assert_eq!(
            budget_status(m(2026, 6), &budgets, &[], &[])[0].planned,
            money(45_000)
        );
    }

    #[test]
    fn rejects_amounts_not_anchored_at_the_start() {
        let err = Budget::new_with_amounts(
            BudgetId::new(1),
            AccountId::new(1),
            CategoryId::new(1),
            vec![BudgetSegment::new(m(2026, 2), money(40_000))],
            m(2026, 1),
            None,
        );
        assert_eq!(err, Err(DomainError::InvalidAmountSegments));
    }

    #[test]
    fn rejects_empty_or_unordered_amounts() {
        let build = |segments: Vec<BudgetSegment>| {
            Budget::new_with_amounts(
                BudgetId::new(1),
                AccountId::new(1),
                CategoryId::new(1),
                segments,
                m(2026, 1),
                None,
            )
        };
        assert_eq!(build(vec![]), Err(DomainError::InvalidAmountSegments));
        // Descending.
        assert_eq!(
            build(vec![
                BudgetSegment::new(m(2026, 1), money(40_000)),
                BudgetSegment::new(m(2025, 12), money(45_000)),
            ]),
            Err(DomainError::InvalidAmountSegments)
        );
        // Duplicated month — a month must resolve to exactly one amount.
        assert_eq!(
            build(vec![
                BudgetSegment::new(m(2026, 1), money(40_000)),
                BudgetSegment::new(m(2026, 1), money(45_000)),
            ]),
            Err(DomainError::InvalidAmountSegments)
        );
    }

    #[test]
    fn rejects_too_many_amount_segments() {
        // Bounds untrusted IPC input, as a rule's history does.
        let segments: Vec<BudgetSegment> = (0..=MAX_AMOUNT_SEGMENTS)
            .map(|i| {
                let month = Month::new(2026 + i32::try_from(i).unwrap(), 1).unwrap();
                BudgetSegment::new(month, money(40_000))
            })
            .collect();
        assert!(matches!(
            Budget::new_with_amounts(
                BudgetId::new(1),
                AccountId::new(1),
                CategoryId::new(1),
                segments,
                m(2026, 1),
                None,
            ),
            Err(DomainError::TooManyAmountSegments { .. })
        ));
    }

    #[test]
    fn amount_history_round_trips_through_json() {
        let original = budget_with(&[(m(2026, 1), 40_000), (m(2026, 6), 45_000)]);
        let json = serde_json::to_string(&original).unwrap();
        assert!(
            json.contains(r#""effective_from":{"year":2026,"month":6}"#),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<Budget>(&json).unwrap(), original);
    }
}
