//! Carry-over ledger math: per-month income/expense totals and the running
//! available balance.
//!
//! Carry-over is a **prefix sum**: the balance at the end of month *M* is the
//! account's `opening_balance` plus the signed total of every entry and
//! recurring occurrence dated within `[anchor .. M]`. A month's `available`
//! figure *is* that end-of-month balance, so it chains into the next month.
//!
//! `opening_balance` is authoritative as of `anchor`; contributions in months
//! **before** `anchor` are ignored (no double counting). These functions are
//! pure and take the account's entries/rules already filtered to one account.
//! They are O(history): the persistence layer may cache per-month aggregates
//! later, but `core` stays simple and correct.

use serde::Serialize;

use crate::domain::entry::{Entry, EntryKind};
use crate::domain::month::Month;
use crate::domain::recurring::RecurringRule;
use crate::money::Money;

/// The budget picture for a single month.
///
/// Output-only: an aggregate the shell sends to the frontend. It deliberately
/// does **not** implement `Deserialize` — `available` is a derived field, and
/// accepting a tampered summary could feed an inconsistent carry-over into later
/// months. Persist the raw entries/rules and re-derive instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MonthSummary {
    /// The month described.
    pub month: Month,
    /// Balance carried in from the end of the previous month.
    pub carry_in: Money,
    /// Total income recorded in the month (positive).
    pub income: Money,
    /// Total expenses recorded in the month (positive).
    pub expenses: Money,
    /// Available to end of month: `carry_in + income - expenses`.
    pub available: Money,
}

/// Income and expense totals (both positive) recorded in `month`, including ad
/// hoc entries and the expansion of recurring rules.
fn totals_in_month(month: Month, entries: &[Entry], rules: &[RecurringRule]) -> (Money, Money) {
    let mut income = Money::zero();
    let mut expenses = Money::zero();

    let mut add = |kind: EntryKind, amount: Money| match kind {
        EntryKind::Income => income = income + amount,
        EntryKind::Expense => expenses = expenses + amount,
    };

    for entry in entries {
        if month.contains(entry.date()) {
            add(entry.kind(), entry.amount());
        }
    }
    for rule in rules {
        for occurrence in rule.expand_in(month) {
            add(occurrence.kind(), occurrence.amount());
        }
    }
    (income, expenses)
}

/// The running balance at the end of `month`.
///
/// Equals `opening_balance` for any month at or before `anchor` with no
/// in-range activity, and `opening_balance + Σ (income − expenses)` over every
/// month in `anchor..=month` otherwise.
#[must_use]
pub fn balance_at_end_of(
    month: Month,
    opening_balance: Money,
    anchor: Month,
    entries: &[Entry],
    rules: &[RecurringRule],
) -> Money {
    if month < anchor {
        return opening_balance;
    }
    let mut balance = opening_balance;
    let mut current = anchor;
    loop {
        let (income, expenses) = totals_in_month(current, entries, rules);
        balance = balance + income - expenses;
        if current == month {
            break;
        }
        current = current.succ();
    }
    balance
}

/// The [`MonthSummary`] for a single month.
#[must_use]
pub fn month_summary(
    month: Month,
    opening_balance: Money,
    anchor: Month,
    entries: &[Entry],
    rules: &[RecurringRule],
) -> MonthSummary {
    if month < anchor {
        return MonthSummary {
            month,
            carry_in: opening_balance,
            income: Money::zero(),
            expenses: Money::zero(),
            available: opening_balance,
        };
    }
    let carry_in = balance_at_end_of(month.pred(), opening_balance, anchor, entries, rules);
    let (income, expenses) = totals_in_month(month, entries, rules);
    let available = carry_in + income - expenses;
    MonthSummary {
        month,
        carry_in,
        income,
        expenses,
        available,
    }
}

/// Contiguous [`MonthSummary`]s for `from..=to`, computing the carry-over chain
/// once (each month's `carry_in` is the previous month's `available`).
///
/// Returns empty if `from > to`.
#[must_use]
pub fn summaries_for_range(
    from: Month,
    to: Month,
    opening_balance: Money,
    anchor: Month,
    entries: &[Entry],
    rules: &[RecurringRule],
) -> Vec<MonthSummary> {
    let mut out = Vec::new();
    if from > to {
        return out;
    }
    let mut carry = balance_at_end_of(from.pred(), opening_balance, anchor, entries, rules);
    let mut current = from;
    loop {
        let summary = if current < anchor {
            MonthSummary {
                month: current,
                carry_in: opening_balance,
                income: Money::zero(),
                expenses: Money::zero(),
                available: opening_balance,
            }
        } else {
            let (income, expenses) = totals_in_month(current, entries, rules);
            let available = carry + income - expenses;
            MonthSummary {
                month: current,
                carry_in: carry,
                income,
                expenses,
                available,
            }
        };
        carry = summary.available;
        out.push(summary);
        if current == to {
            break;
        }
        current = current.succ();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{balance_at_end_of, month_summary, summaries_for_range};
    use crate::domain::entry::{Entry, EntryKind};
    use crate::domain::ids::{AccountId, EntryId, RecurringRuleId};
    use crate::domain::month::Month;
    use crate::domain::recurring::{FreqUnit, Frequency, RecurringRule, RuleEnd};
    use crate::money::Money;
    use time::{Date, Month as TMonth};

    fn m(y: i32, mo: u8) -> Month {
        Month::new(y, mo).unwrap()
    }

    fn entry(id: u64, minor: i64, kind: EntryKind, y: i32, mo: TMonth, d: u8) -> Entry {
        Entry::new(
            EntryId::new(id),
            AccountId::new(1),
            Money::from_minor_units(minor, 2),
            kind,
            Date::from_calendar_date(y, mo, d).unwrap(),
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn opening_balance_carries_when_no_activity() {
        let opening = Money::from_minor_units(10_000, 2); // 100.00
        let anchor = m(2026, 1);
        for month in [m(2026, 1), m(2026, 2), m(2026, 6)] {
            let s = month_summary(month, opening, anchor, &[], &[]);
            assert_eq!(s.carry_in, opening);
            assert_eq!(s.available, opening);
        }
    }

    #[test]
    fn carries_surplus_into_next_month() {
        let anchor = m(2026, 1);
        let entries = [
            entry(1, 50_000, EntryKind::Income, 2026, TMonth::January, 1), // +500
            entry(2, 20_000, EntryKind::Expense, 2026, TMonth::January, 5), // -200
            entry(3, 5_000, EntryKind::Expense, 2026, TMonth::February, 3), // -50
        ];
        let jan = month_summary(m(2026, 1), Money::zero(), anchor, &entries, &[]);
        assert_eq!(jan.income, Money::from_minor_units(50_000, 2));
        assert_eq!(jan.expenses, Money::from_minor_units(20_000, 2));
        assert_eq!(jan.available, Money::from_minor_units(30_000, 2)); // 300

        let feb = month_summary(m(2026, 2), Money::zero(), anchor, &entries, &[]);
        assert_eq!(feb.carry_in, Money::from_minor_units(30_000, 2)); // 300
        assert_eq!(feb.available, Money::from_minor_units(25_000, 2)); // 250
    }

    #[test]
    fn overspend_yields_negative_carry() {
        let anchor = m(2026, 1);
        let entries = [entry(
            1,
            10_000,
            EntryKind::Expense,
            2026,
            TMonth::January,
            5,
        )];
        let jan = month_summary(m(2026, 1), Money::zero(), anchor, &entries, &[]);
        assert!(jan.available.is_negative());
        let feb = month_summary(m(2026, 2), Money::zero(), anchor, &entries, &[]);
        assert_eq!(feb.carry_in, jan.available);
    }

    #[test]
    fn recurring_and_ad_hoc_combine() {
        let anchor = m(2026, 1);
        // Salary 2000 on the 1st, monthly, forever.
        let salary = RecurringRule::new(
            RecurringRuleId::new(1),
            AccountId::new(1),
            Money::from_minor_units(200_000, 2),
            EntryKind::Income,
            None,
            None,
            Date::from_calendar_date(2026, TMonth::January, 1).unwrap(),
            RuleEnd::Never,
            Frequency::new(FreqUnit::Monthly, 1).unwrap(),
        )
        .unwrap();
        let entries = [entry(
            1,
            30_000,
            EntryKind::Expense,
            2026,
            TMonth::January,
            10,
        )]; // -300

        let jan = month_summary(
            m(2026, 1),
            Money::zero(),
            anchor,
            &entries,
            std::slice::from_ref(&salary),
        );
        assert_eq!(jan.income, Money::from_minor_units(200_000, 2)); // 2000 from rule
        assert_eq!(jan.available, Money::from_minor_units(170_000, 2)); // 1700
        let feb = month_summary(
            m(2026, 2),
            Money::zero(),
            anchor,
            &entries,
            std::slice::from_ref(&salary),
        );
        // carry 1700 + 2000 salary, no Feb expenses = 3700
        assert_eq!(feb.available, Money::from_minor_units(370_000, 2));
    }

    #[test]
    fn entries_before_anchor_are_ignored() {
        let anchor = m(2026, 3);
        let entries = [entry(
            1,
            99_999,
            EntryKind::Income,
            2026,
            TMonth::January,
            15,
        )];
        let summary = month_summary(
            m(2026, 3),
            Money::from_minor_units(10_000, 2),
            anchor,
            &entries,
            &[],
        );
        assert_eq!(summary.carry_in, Money::from_minor_units(10_000, 2));
        assert_eq!(summary.available, Money::from_minor_units(10_000, 2));
    }

    #[test]
    fn chain_identity_walk_equals_prefix_sum() {
        let anchor = m(2026, 1);
        let entries = [
            entry(1, 50_000, EntryKind::Income, 2026, TMonth::January, 1),
            entry(2, 20_000, EntryKind::Expense, 2026, TMonth::February, 5),
            entry(3, 10_000, EntryKind::Income, 2026, TMonth::March, 9),
        ];
        let range =
            summaries_for_range(m(2026, 1), m(2026, 3), Money::zero(), anchor, &entries, &[]);
        for s in &range {
            assert_eq!(
                s.available,
                balance_at_end_of(s.month, Money::zero(), anchor, &entries, &[]),
                "walked available must equal direct prefix-sum balance for {:?}",
                s.month
            );
        }
    }

    #[test]
    fn carry_chains_across_year_boundary() {
        let anchor = m(2026, 12);
        let entries = [entry(
            1,
            40_000,
            EntryKind::Income,
            2026,
            TMonth::December,
            1,
        )];
        let dec = month_summary(m(2026, 12), Money::zero(), anchor, &entries, &[]);
        let jan = month_summary(m(2027, 1), Money::zero(), anchor, &entries, &[]);
        assert_eq!(dec.available, Money::from_minor_units(40_000, 2));
        assert_eq!(jan.carry_in, dec.available);
    }
}
