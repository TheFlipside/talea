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
//!
//! [`balance_as_of`] is the same prefix sum at **day** granularity: it stops at
//! a date instead of a month's end. Evaluated for today, it is what the real
//! account (a bank statement) should show if every transaction was recorded —
//! a reconciliation aid. See `docs/DESIGN.md` §13.
//!
//! [`Budget`]s ride **on top of** that chain without disturbing it. They add two
//! derived figures — `reserved` (what this month's category budgets still hold
//! back) and `free` (`available − reserved`) — but never touch `available`, so
//! [`balance_at_end_of`] and the carry-over walk are budget-independent by
//! construction. See `docs/DESIGN.md` §12.

use serde::Serialize;
use time::Date;

use crate::domain::budget::{reserved_in, Budget};
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
    ///
    /// Budget-independent: this is the account's real end-of-month balance, and
    /// it is what chains into the next month's `carry_in`.
    pub available: Money,
    /// Still held back by this month's category budgets:
    /// `Σ max(0, planned − actual)`. Zero when the account has no budgets, in
    /// which case `free == available`.
    pub reserved: Money,
    /// Free to spend on anything not already budgeted: `available − reserved`.
    ///
    /// The budget ring and the home-screen widget follow this figure — it is the
    /// answer to "can I spend this?" — but it is a *view*, never the chain: an
    /// unspent reservation expires at month end and its money carries over as
    /// ordinary surplus.
    pub free: Money,
}

/// Income and expense totals (both positive) recorded in `month`, including ad
/// hoc entries and the expansion of recurring rules.
fn totals_in_month(month: Month, entries: &[Entry], rules: &[RecurringRule]) -> (Money, Money) {
    totals_in_month_through(month, month.last_day(), entries, rules)
}

/// Like [`totals_in_month`], but only counting what is dated **on or before**
/// `through` — the partial month that [`balance_as_of`] needs. With `through`
/// at the month's last day, this is the whole month.
fn totals_in_month_through(
    month: Month,
    through: Date,
    entries: &[Entry],
    rules: &[RecurringRule],
) -> (Money, Money) {
    let mut income = Money::zero();
    let mut expenses = Money::zero();

    let mut add = |kind: EntryKind, amount: Money| match kind {
        EntryKind::Income => income = income + amount,
        EntryKind::Expense => expenses = expenses + amount,
    };

    for entry in entries {
        if month.contains(entry.date()) && entry.date() <= through {
            add(entry.kind(), entry.amount());
        }
    }
    for rule in rules {
        for occurrence in rule.expand_in(month) {
            if occurrence.date() <= through {
                add(occurrence.kind(), occurrence.amount());
            }
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

/// The running balance at the end of `date`: the opening balance plus every
/// entry and recurring occurrence dated on or before it.
///
/// The day-granular counterpart of [`balance_at_end_of`] (on a month's last day
/// the two agree). Evaluated for today, it is the figure the real account should
/// show if every transaction was recorded: anything dated later in the month —
/// a pre-recorded bill, an upcoming recurring occurrence — is not counted yet,
/// and a skipped occurrence is never counted. Like `available`, it is
/// budget-independent.
///
/// Returns `opening_balance` for any date before `anchor`'s month, for the same
/// reason [`balance_at_end_of`] does: the opening balance is authoritative as of
/// the anchor, and nothing before it is counted.
#[must_use]
pub fn balance_as_of(
    date: Date,
    opening_balance: Money,
    anchor: Month,
    entries: &[Entry],
    rules: &[RecurringRule],
) -> Money {
    let month = Month::containing(date);
    // `Month::containing` skips `Month::new`'s `1..=9999` year check, but a date
    // before year 1 never gets past here: every anchor is a validated month, so
    // such a date is always before it.
    if month < anchor {
        return opening_balance;
    }
    let carry_in = balance_at_end_of(month.pred(), opening_balance, anchor, entries, rules);
    let (income, expenses) = totals_in_month_through(month, date, entries, rules);
    carry_in + income - expenses
}

/// A month at or before `anchor` with no in-range activity: the opening balance
/// carries straight through, and budgets are inert (there is nothing to hold
/// back out of a month the ledger does not yet cover).
fn pre_anchor_summary(month: Month, opening_balance: Money) -> MonthSummary {
    MonthSummary {
        month,
        carry_in: opening_balance,
        income: Money::zero(),
        expenses: Money::zero(),
        available: opening_balance,
        reserved: Money::zero(),
        free: opening_balance,
    }
}

/// The [`MonthSummary`] for a single month.
///
/// `budgets` is taken by value rather than defaulted to empty deliberately:
/// omitting them yields `free == available`, a silently wrong answer wherever
/// the ring reads it, so a missed call site must be a compile error.
#[must_use]
pub fn month_summary(
    month: Month,
    opening_balance: Money,
    anchor: Month,
    entries: &[Entry],
    rules: &[RecurringRule],
    budgets: &[Budget],
) -> MonthSummary {
    if month < anchor {
        return pre_anchor_summary(month, opening_balance);
    }
    let carry_in = balance_at_end_of(month.pred(), opening_balance, anchor, entries, rules);
    let (income, expenses) = totals_in_month(month, entries, rules);
    let available = carry_in + income - expenses;
    let reserved = reserved_in(month, budgets, entries, rules);
    MonthSummary {
        month,
        carry_in,
        income,
        expenses,
        available,
        reserved,
        free: available - reserved,
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
    budgets: &[Budget],
) -> Vec<MonthSummary> {
    let mut out = Vec::new();
    if from > to {
        return out;
    }
    let mut carry = balance_at_end_of(from.pred(), opening_balance, anchor, entries, rules);
    let mut current = from;
    loop {
        let summary = if current < anchor {
            pre_anchor_summary(current, opening_balance)
        } else {
            let (income, expenses) = totals_in_month(current, entries, rules);
            let available = carry + income - expenses;
            let reserved = reserved_in(current, budgets, entries, rules);
            MonthSummary {
                month: current,
                carry_in: carry,
                income,
                expenses,
                available,
                reserved,
                free: available - reserved,
            }
        };
        // `available`, never `free`: the chain is the real balance. Carrying
        // `free` would re-subtract the reservation every later month and diverge
        // from `balance_at_end_of`.
        carry = summary.available;
        out.push(summary);
        if current == to {
            break;
        }
        current = current.succ();
    }
    out
}

/// Combines several accounts' [`MonthSummary`]s for the same month into one, by
/// summing each field. Used for **summary accounts** (`docs/DESIGN.md` §11),
/// whose figures are the total across their members.
///
/// The caller guarantees the members share a currency (so the sums are
/// meaningful — `core` never converts). An empty `parts` slice yields an all-zero
/// summary for `month`.
///
/// Summing `reserved` per member is the *correct* semantic, not merely the
/// convenient one: one member overspending its Groceries budget must not cancel
/// another member's Groceries reservation — separate accounts are separate pots.
/// And since `Σ(availableᵢ − reservedᵢ) = Σavailableᵢ − Σreservedᵢ`, the identity
/// `free == available − reserved` survives the field-wise sum exactly.
#[must_use]
pub fn combine_summaries(month: Month, parts: &[MonthSummary]) -> MonthSummary {
    let mut combined = MonthSummary {
        month,
        carry_in: Money::zero(),
        income: Money::zero(),
        expenses: Money::zero(),
        available: Money::zero(),
        reserved: Money::zero(),
        free: Money::zero(),
    };
    for part in parts {
        combined.carry_in = combined.carry_in + part.carry_in;
        combined.income = combined.income + part.income;
        combined.expenses = combined.expenses + part.expenses;
        combined.available = combined.available + part.available;
        combined.reserved = combined.reserved + part.reserved;
        combined.free = combined.free + part.free;
    }
    combined
}

#[cfg(test)]
mod tests {
    use super::{
        balance_as_of, balance_at_end_of, combine_summaries, month_summary, summaries_for_range,
    };
    use crate::domain::budget::Budget;
    use crate::domain::entry::{Entry, EntryKind};
    use crate::domain::ids::{AccountId, BudgetId, CategoryId, EntryId, RecurringRuleId};
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

    /// An expense of `minor` in January 2026, classified under category 1.
    fn spend(id: u64, minor: i64) -> Entry {
        Entry::new(
            EntryId::new(id),
            AccountId::new(1),
            Money::from_minor_units(minor, 2),
            EntryKind::Expense,
            Date::from_calendar_date(2026, TMonth::January, 10).unwrap(),
            None,
            Some(CategoryId::new(1)),
        )
        .unwrap()
    }

    /// An open-ended budget of `minor` on category 1, from January 2026.
    fn budget(minor: i64) -> Budget {
        Budget::new(
            BudgetId::new(1),
            AccountId::new(1),
            CategoryId::new(1),
            Money::from_minor_units(minor, 2),
            m(2026, 1),
            None,
        )
        .unwrap()
    }

    #[test]
    fn combine_summaries_sums_each_field() {
        let anchor = m(2026, 1);
        let a = month_summary(
            m(2026, 1),
            Money::from_minor_units(10_000, 2), // 100.00 opening
            anchor,
            &[entry(
                1,
                50_000,
                EntryKind::Income,
                2026,
                TMonth::January,
                1,
            )], // +500
            &[],
            &[],
        );
        let b = month_summary(
            m(2026, 1),
            Money::from_minor_units(2_000, 2), // 20.00 opening
            anchor,
            &[entry(
                2,
                30_000,
                EntryKind::Expense,
                2026,
                TMonth::January,
                2,
            )], // -300
            &[],
            &[],
        );
        let combined = combine_summaries(m(2026, 1), &[a.clone(), b.clone()]);
        assert_eq!(combined.month, m(2026, 1));
        assert_eq!(combined.carry_in, a.carry_in + b.carry_in); // 120.00
        assert_eq!(combined.income, Money::from_minor_units(50_000, 2)); // 500.00
        assert_eq!(combined.expenses, Money::from_minor_units(30_000, 2)); // 300.00
        assert_eq!(combined.available, a.available + b.available);
    }

    #[test]
    fn combine_summaries_of_nothing_is_zero() {
        let combined = combine_summaries(m(2026, 4), &[]);
        assert_eq!(combined.carry_in, Money::zero());
        assert_eq!(combined.income, Money::zero());
        assert_eq!(combined.expenses, Money::zero());
        assert_eq!(combined.available, Money::zero());
    }

    #[test]
    fn opening_balance_carries_when_no_activity() {
        let opening = Money::from_minor_units(10_000, 2); // 100.00
        let anchor = m(2026, 1);
        for month in [m(2026, 1), m(2026, 2), m(2026, 6)] {
            let s = month_summary(month, opening, anchor, &[], &[], &[]);
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
        let jan = month_summary(m(2026, 1), Money::zero(), anchor, &entries, &[], &[]);
        assert_eq!(jan.income, Money::from_minor_units(50_000, 2));
        assert_eq!(jan.expenses, Money::from_minor_units(20_000, 2));
        assert_eq!(jan.available, Money::from_minor_units(30_000, 2)); // 300

        let feb = month_summary(m(2026, 2), Money::zero(), anchor, &entries, &[], &[]);
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
        let jan = month_summary(m(2026, 1), Money::zero(), anchor, &entries, &[], &[]);
        assert!(jan.available.is_negative());
        let feb = month_summary(m(2026, 2), Money::zero(), anchor, &entries, &[], &[]);
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
            &[],
        );
        assert_eq!(jan.income, Money::from_minor_units(200_000, 2)); // 2000 from rule
        assert_eq!(jan.available, Money::from_minor_units(170_000, 2)); // 1700
        let feb = month_summary(
            m(2026, 2),
            Money::zero(),
            anchor,
            &entries,
            std::slice::from_ref(&salary),
            &[],
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
        // Run it *with* a budget in play: `balance_at_end_of` knows nothing about
        // budgets, so this equality is what proves `available` stays
        // budget-independent all the way along the chain.
        let budgets = [budget(30_000)];
        let range = summaries_for_range(
            m(2026, 1),
            m(2026, 3),
            Money::zero(),
            anchor,
            &entries,
            &[],
            &budgets,
        );
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
    fn reserves_unspent_budget_without_touching_available() {
        let anchor = m(2026, 1);
        let entries = [
            entry(1, 200_000, EntryKind::Income, 2026, TMonth::January, 1), // +2000
            spend(2, 15_000),                                               // -150 groceries
        ];
        let budgets = [budget(40_000)]; // 400 groceries

        let with = month_summary(m(2026, 1), Money::zero(), anchor, &entries, &[], &budgets);
        let without = month_summary(m(2026, 1), Money::zero(), anchor, &entries, &[], &[]);

        // The budget moves no money: every ledger figure is untouched.
        assert_eq!(with.carry_in, without.carry_in);
        assert_eq!(with.income, without.income);
        assert_eq!(with.expenses, without.expenses);
        assert_eq!(with.available, without.available);

        // 400 planned − 150 already spent → 250 still held back.
        assert_eq!(with.reserved, Money::from_minor_units(25_000, 2));
        assert_eq!(with.free, with.available - with.reserved);
        assert_eq!(with.free, Money::from_minor_units(160_000, 2)); // 1600
    }

    #[test]
    fn free_equals_available_when_there_are_no_budgets() {
        let anchor = m(2026, 1);
        let entries = [entry(
            1,
            50_000,
            EntryKind::Income,
            2026,
            TMonth::January,
            1,
        )];
        let s = month_summary(m(2026, 1), Money::zero(), anchor, &entries, &[], &[]);
        assert_eq!(s.reserved, Money::zero());
        assert_eq!(s.free, s.available);
    }

    #[test]
    fn range_chains_available_not_free() {
        let anchor = m(2026, 1);
        let entries = [entry(
            1,
            200_000,
            EntryKind::Income,
            2026,
            TMonth::January,
            1,
        )];
        let budgets = [budget(40_000)]; // fully unspent → reserves 400 every month
        let range = summaries_for_range(
            m(2026, 1),
            m(2026, 2),
            Money::zero(),
            anchor,
            &entries,
            &[],
            &budgets,
        );
        assert_eq!(range[0].reserved, Money::from_minor_units(40_000, 2));
        assert!(range[0].free < range[0].available);
        // February carries January's *available*, not its `free` — the 400 the
        // user did not spend on groceries is still their money.
        assert_eq!(range[1].carry_in, range[0].available);
        assert_ne!(range[1].carry_in, range[0].free);
    }

    #[test]
    fn combine_summaries_preserves_free_equals_available_minus_reserved() {
        let anchor = m(2026, 1);
        // Member A: 400 budgeted, 100 spent → reserves 300.
        let a = month_summary(
            m(2026, 1),
            Money::from_minor_units(100_000, 2),
            anchor,
            &[spend(1, 10_000)],
            &[],
            &[budget(40_000)],
        );
        // Member B: 200 budgeted, 300 spent → overspent, reserves nothing.
        let b = month_summary(
            m(2026, 1),
            Money::from_minor_units(100_000, 2),
            anchor,
            &[spend(2, 30_000)],
            &[],
            &[budget(20_000)],
        );
        let combined = combine_summaries(m(2026, 1), &[a.clone(), b.clone()]);

        assert_eq!(combined.reserved, a.reserved + b.reserved);
        assert_eq!(combined.free, combined.available - combined.reserved);
        // Summing per member, not merging by category: B's overspend must not
        // cancel A's reservation. Re-deriving from merged totals would give
        // max(0, 600 − 400) = 200 here, which would understate what A is holding.
        assert_eq!(combined.reserved, Money::from_minor_units(30_000, 2));
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
        let dec = month_summary(m(2026, 12), Money::zero(), anchor, &entries, &[], &[]);
        let jan = month_summary(m(2027, 1), Money::zero(), anchor, &entries, &[], &[]);
        assert_eq!(dec.available, Money::from_minor_units(40_000, 2));
        assert_eq!(jan.carry_in, dec.available);
    }

    /// A day in 2026, for `balance_as_of` cut-offs.
    fn day(mo: TMonth, d: u8) -> Date {
        Date::from_calendar_date(2026, mo, d).unwrap()
    }

    /// A never-ending rule of `minor`, from `start`, every `interval` `unit`s.
    fn rule(
        minor: i64,
        kind: EntryKind,
        start: Date,
        unit: FreqUnit,
        interval: u32,
    ) -> RecurringRule {
        RecurringRule::new(
            RecurringRuleId::new(1),
            AccountId::new(1),
            Money::from_minor_units(minor, 2),
            kind,
            None,
            None,
            start,
            RuleEnd::Never,
            Frequency::new(unit, interval).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn balance_as_of_counts_only_activity_up_to_the_date() {
        let anchor = m(2026, 1);
        let entries = [
            entry(1, 100_000, EntryKind::Income, 2026, TMonth::January, 5), // +1000
            entry(2, 20_000, EntryKind::Expense, 2026, TMonth::January, 10), // -200
            entry(3, 30_000, EntryKind::Expense, 2026, TMonth::January, 20), // -300
        ];
        let as_of = |d| {
            balance_as_of(
                day(TMonth::January, d),
                Money::zero(),
                anchor,
                &entries,
                &[],
            )
        };

        assert_eq!(as_of(4), Money::zero());
        assert_eq!(as_of(9), Money::from_minor_units(100_000, 2));
        // The date itself counts: an expense recorded today is already spent.
        assert_eq!(as_of(10), Money::from_minor_units(80_000, 2));
        // The 20th's expense is still ahead: today's balance sits above what the
        // month will end on, which is exactly the gap a bank statement shows.
        assert_eq!(as_of(19), Money::from_minor_units(80_000, 2));
        assert_eq!(as_of(31), Money::from_minor_units(50_000, 2));
    }

    #[test]
    fn balance_as_of_counts_a_recurring_occurrence_from_its_day() {
        let anchor = m(2026, 1);
        let opening = Money::from_minor_units(200_000, 2); // 2000
        let rent = rule(
            70_000,
            EntryKind::Expense,
            day(TMonth::January, 15),
            FreqUnit::Monthly,
            1,
        );
        let as_of = |mo, d| {
            balance_as_of(
                day(mo, d),
                opening,
                anchor,
                &[],
                std::slice::from_ref(&rent),
            )
        };

        assert_eq!(as_of(TMonth::January, 14), opening);
        assert_eq!(
            as_of(TMonth::January, 15),
            Money::from_minor_units(130_000, 2)
        );
        // February carries January's rent in, then books its own on the 15th.
        assert_eq!(
            as_of(TMonth::February, 14),
            Money::from_minor_units(130_000, 2)
        );
        assert_eq!(
            as_of(TMonth::February, 15),
            Money::from_minor_units(60_000, 2)
        );
    }

    #[test]
    fn balance_as_of_a_month_end_equals_balance_at_end_of() {
        let anchor = m(2026, 1);
        let opening = Money::from_minor_units(10_000, 2);
        // Activity on the very first and last days, to pin both month edges.
        let entries = [
            entry(1, 50_000, EntryKind::Income, 2026, TMonth::January, 31),
            entry(2, 20_000, EntryKind::Expense, 2026, TMonth::February, 28),
            entry(3, 10_000, EntryKind::Income, 2026, TMonth::March, 1),
        ];
        // Fortnightly, so occurrences land on a different day every month.
        let rules = [rule(
            2_500,
            EntryKind::Expense,
            day(TMonth::January, 3),
            FreqUnit::Weekly,
            2,
        )];
        for month in [m(2026, 1), m(2026, 2), m(2026, 3), m(2026, 4)] {
            assert_eq!(
                balance_as_of(month.last_day(), opening, anchor, &entries, &rules),
                balance_at_end_of(month, opening, anchor, &entries, &rules),
                "day-granular balance must agree with the month chain at {month:?}",
            );
        }
    }

    #[test]
    fn balance_as_of_before_the_anchor_is_the_opening_balance() {
        let anchor = m(2026, 3);
        let opening = Money::from_minor_units(10_000, 2);
        // Dated before the anchor, so the opening balance already accounts for it.
        let entries = [entry(
            1,
            99_999,
            EntryKind::Income,
            2026,
            TMonth::January,
            15,
        )];
        let as_of = |mo, d| balance_as_of(day(mo, d), opening, anchor, &entries, &[]);

        assert_eq!(as_of(TMonth::February, 20), opening);
        assert_eq!(as_of(TMonth::March, 1), opening);
    }

    #[test]
    fn balance_as_of_survives_the_most_extreme_dates() {
        // Without `time`'s `large-dates` feature, years stop at ±9999, which is
        // what bounds the walk below. Enabling it anywhere in the build must
        // fail here rather than silently lift that bound.
        assert_eq!(Date::MAX.year(), 9999);

        let anchor = m(2026, 1);
        let opening = Money::from_minor_units(10_000, 2);
        let entries = [entry(1, 5_000, EntryKind::Income, 2026, TMonth::March, 1)];
        // Year 0 and below: before any anchor, so the opening balance comes back
        // without any month arithmetic on a year `Month::new` would reject.
        let year_zero = Date::from_calendar_date(0, TMonth::December, 31).unwrap();
        for date in [Date::MIN, year_zero] {
            assert_eq!(balance_as_of(date, opening, anchor, &entries, &[]), opening);
        }
        // The far end walks the whole chain without panicking.
        assert_eq!(
            balance_as_of(Date::MAX, opening, anchor, &entries, &[]),
            Money::from_minor_units(15_000, 2)
        );
    }
}
