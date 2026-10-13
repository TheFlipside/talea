//! The Talea domain model — a **monthly cashflow ledger with carry-over**.
//!
//! Not envelope budgeting: each month's *available* figure is
//! `carry_in + income − expenses`, and a month's ending balance chains into the
//! next (per account). See `docs/DESIGN.md` for the full model.
//!
//! - [`Account`] — a tracked account with a fixed [`Currency`] and an opening
//!   balance anchored to a [`Month`].
//! - [`Category`] — a global, descriptive classification with a [`CategoryIcon`].
//! - [`Entry`] — a recorded income/expense ([`EntryKind`]); amount is a positive
//!   magnitude, sign derived from the kind.
//! - [`RecurringRule`] — an entry template with a [`Frequency`] and [`RuleEnd`],
//!   expanded into occurrences per month.
//! - [`Budget`] — a planned monthly amount for one [`Category`], consumed by
//!   that month's spending in it ([`budget_status`], [`reserved_in`]).
//! - [`ledger`] — carry-over math: [`MonthSummary`], [`month_summary`],
//!   [`summaries_for_range`], [`balance_at_end_of`], and the day-granular
//!   [`balance_as_of`].
//!
//! Every type with invariants is built through a validating constructor and
//! deserializes through that same path, so malformed input (over IPC or
//! storage) is rejected rather than silently accepted.

pub mod account;
pub mod budget;
pub mod category;
pub mod entry;
pub mod error;
pub mod ids;
pub mod ledger;
pub mod month;
pub mod recurring;
pub mod stats;

pub(crate) mod date;

pub use account::{Account, AccountKind, Currency};
pub use budget::{budget_status, reserved_in, Budget, BudgetSegment, BudgetStatus};
pub use category::{Category, CategoryIcon};
pub use entry::{Entry, EntryKind};
pub use error::{DomainError, MAX_LABEL_LEN, MAX_NOTE_LEN};
pub use ids::{AccountId, BudgetId, CategoryId, EntryId, RecurringRuleId};
pub use ledger::{
    balance_as_of, balance_at_end_of, combine_summaries, month_summary, summaries_for_range,
    MonthSummary,
};
pub use month::Month;
pub use recurring::{AmountSegment, FreqUnit, Frequency, RecurringRule, RuleEnd, VirtualEntry};
pub use stats::{expenses_by_category, CategoryExpense};
