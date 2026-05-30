//! Category breakdown of a month's expenses, for the statistics screen.
//!
//! Pure aggregation over the same inputs the [`ledger`](crate::domain::ledger)
//! uses: a month's ad hoc entries plus the expansion of recurring rules. Income
//! is ignored — the stats screen reports where money *went*. Expenses with no
//! category collect under a single `None` bucket, which the shell presents as
//! "Other".

use serde::Serialize;

use crate::domain::entry::{Entry, EntryKind};
use crate::domain::ids::CategoryId;
use crate::domain::month::Month;
use crate::domain::recurring::RecurringRule;
use crate::money::Money;

/// One category's total expense within a month (a positive magnitude).
///
/// Output-only: like [`MonthSummary`](crate::domain::MonthSummary) it
/// deliberately does **not** implement `Deserialize` — totals are derived from
/// the raw entries/rules, never accepted from outside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategoryExpense {
    /// The category these expenses are classified under, or `None` for
    /// uncategorized expenses (the shell presents these as "Other").
    pub category_id: Option<CategoryId>,
    /// Total expense recorded under this category in the month (positive).
    pub total: Money,
}

/// Totals a month's **expenses** grouped by category, descending by amount.
///
/// Includes ad hoc entries and recurring-rule expansions for `month`; income is
/// ignored. Expenses with no category collect under `category_id: None`. Ties
/// are broken by category id (the `None` bucket last) so the order is fully
/// deterministic regardless of input ordering.
#[must_use]
pub fn expenses_by_category(
    month: Month,
    entries: &[Entry],
    rules: &[RecurringRule],
) -> Vec<CategoryExpense> {
    // A category count is small, so a linear-probe Vec is simpler than a map and
    // keeps insertion order available for the deterministic tie-break below.
    let mut totals: Vec<(Option<CategoryId>, Money)> = Vec::new();
    let mut add = |category_id: Option<CategoryId>, amount: Money| {
        if let Some(slot) = totals.iter_mut().find(|(id, _)| *id == category_id) {
            // `Money`'s `+` panics on overflow by design (see DESIGN.md §9); the
            // per-entry amount cap keeps any category total far below `Decimal`'s
            // range, so this is unreachable in practice.
            slot.1 = slot.1 + amount;
        } else {
            totals.push((category_id, amount));
        }
    };

    for entry in entries {
        if entry.kind() == EntryKind::Expense && month.contains(entry.date()) {
            add(entry.category_id(), entry.amount());
        }
    }
    for rule in rules {
        for occurrence in rule.expand_in(month) {
            if occurrence.kind() == EntryKind::Expense {
                add(occurrence.category_id(), occurrence.amount());
            }
        }
    }

    // Descending by total; tie-break by category id with the `None` (Other)
    // bucket last, so equal totals never reorder run to run.
    totals.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| match (a.0, b.0) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
    });

    totals
        .into_iter()
        .map(|(category_id, total)| CategoryExpense { category_id, total })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::expenses_by_category;
    use crate::domain::entry::{Entry, EntryKind};
    use crate::domain::ids::{AccountId, CategoryId, EntryId, RecurringRuleId};
    use crate::domain::month::Month;
    use crate::domain::recurring::{FreqUnit, Frequency, RecurringRule, RuleEnd};
    use crate::money::Money;
    use time::{Date, Month as TMonth};

    fn m(y: i32, mo: u8) -> Month {
        Month::new(y, mo).unwrap()
    }

    fn entry(
        id: u64,
        minor: i64,
        kind: EntryKind,
        d: u8,
        category_id: Option<CategoryId>,
    ) -> Entry {
        Entry::new(
            EntryId::new(id),
            AccountId::new(1),
            Money::from_minor_units(minor, 2),
            kind,
            Date::from_calendar_date(2026, TMonth::January, d).unwrap(),
            None,
            category_id,
        )
        .unwrap()
    }

    #[test]
    fn groups_expenses_by_category_ignoring_income() {
        let cat = |id| Some(CategoryId::new(id));
        let entries = [
            entry(1, 10_000, EntryKind::Expense, 3, cat(1)), // 100 → cat 1
            entry(2, 5_000, EntryKind::Expense, 4, cat(1)),  // 50  → cat 1
            entry(3, 8_000, EntryKind::Expense, 5, cat(2)),  // 80  → cat 2
            entry(4, 99_999, EntryKind::Income, 6, cat(2)),  // income: ignored
        ];
        let out = expenses_by_category(m(2026, 1), &entries, &[]);
        assert_eq!(out.len(), 2);
        // Descending by total: cat 1 (150) before cat 2 (80).
        assert_eq!(out[0].category_id, cat(1));
        assert_eq!(out[0].total, Money::from_minor_units(15_000, 2));
        assert_eq!(out[1].category_id, cat(2));
        assert_eq!(out[1].total, Money::from_minor_units(8_000, 2));
    }

    #[test]
    fn uncategorized_expenses_collect_under_none() {
        let entries = [
            entry(1, 3_000, EntryKind::Expense, 3, None),
            entry(2, 2_000, EntryKind::Expense, 4, None),
            entry(3, 1_000, EntryKind::Expense, 5, Some(CategoryId::new(7))),
        ];
        let out = expenses_by_category(m(2026, 1), &entries, &[]);
        assert_eq!(out.len(), 2);
        // None bucket (50) leads on amount; sums both uncategorized entries.
        assert_eq!(out[0].category_id, None);
        assert_eq!(out[0].total, Money::from_minor_units(5_000, 2));
        assert_eq!(out[1].category_id, Some(CategoryId::new(7)));
    }

    #[test]
    fn includes_recurring_expansions() {
        let rule = RecurringRule::new(
            RecurringRuleId::new(1),
            AccountId::new(1),
            Money::from_minor_units(4_000, 2), // 40 monthly expense
            EntryKind::Expense,
            None,
            Some(CategoryId::new(3)),
            Date::from_calendar_date(2026, TMonth::January, 1).unwrap(),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        )
        .unwrap();
        let entries = [entry(
            1,
            1_000,
            EntryKind::Expense,
            5,
            Some(CategoryId::new(3)),
        )];
        let out = expenses_by_category(m(2026, 1), &entries, std::slice::from_ref(&rule));
        // 10 ad hoc + 40 recurring, same category → 50.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].category_id, Some(CategoryId::new(3)));
        assert_eq!(out[0].total, Money::from_minor_units(5_000, 2));
    }

    #[test]
    fn empty_when_no_expenses() {
        let entries = [entry(1, 9_000, EntryKind::Income, 3, None)];
        assert!(expenses_by_category(m(2026, 1), &entries, &[]).is_empty());
    }

    #[test]
    fn equal_totals_break_ties_deterministically() {
        let cat = |id| Some(CategoryId::new(id));
        // Two categories with identical totals, inserted high-id first.
        let entries = [
            entry(1, 5_000, EntryKind::Expense, 3, cat(9)),
            entry(2, 5_000, EntryKind::Expense, 4, cat(2)),
            entry(3, 5_000, EntryKind::Expense, 5, None),
        ];
        let out = expenses_by_category(m(2026, 1), &entries, &[]);
        // Equal totals → ascending id, None last.
        assert_eq!(out[0].category_id, cat(2));
        assert_eq!(out[1].category_id, cat(9));
        assert_eq!(out[2].category_id, None);
    }
}
