//! Domain-level validation errors and the limits they enforce.

use thiserror::Error;
use time::Date;

use crate::domain::month::Month;
use crate::money::{Money, MoneyError};

/// Maximum length (in characters) of a free-text note on an entry or rule.
pub const MAX_NOTE_LEN: usize = 1_000;

/// Maximum length (in characters) of a category label.
pub const MAX_LABEL_LEN: usize = 200;

/// Maximum number of amount segments (the base plus later breakpoints) a single
/// recurring rule may carry. Bounds untrusted IPC input so a crafted rule cannot
/// inflate the `rule_amount` table or the per-occurrence amount lookup.
pub const MAX_AMOUNT_SEGMENTS: usize = 100;

/// Maximum number of member accounts a summary account may aggregate. Bounds
/// untrusted IPC input so a crafted summary can't force an unbounded fan-out of
/// per-member ledger scans on every read.
pub const MAX_SUMMARY_MEMBERS: usize = 50;

/// Largest magnitude of any money term the ledger sums — a single entry, rule
/// or budget amount, or an account's opening balance: one quadrillion, in whole
/// currency units. With every term capped, overflowing `Decimal::MAX` (~7.9e28)
/// would take some 8e13 maximal terms, far beyond any real ledger. That bound,
/// not a proof, is what keeps the ledger's panic-on-overflow arithmetic safe.
const MAX_AMOUNT_UNITS: i64 = 1_000_000_000_000_000;

/// [`MAX_AMOUNT_UNITS`] as money (whole units, scale 0), shared by both
/// validators so the two bounds can't drift apart.
fn amount_ceiling() -> Money {
    Money::from_minor_units(MAX_AMOUNT_UNITS, 0)
}

/// Validates an entry/rule amount: a positive magnitude within
/// [`MAX_AMOUNT_UNITS`].
///
/// # Errors
///
/// [`DomainError::NonPositiveAmount`] if zero/negative;
/// [`DomainError::AmountTooLarge`] if it exceeds the ceiling.
pub(crate) fn validate_amount(amount: Money) -> Result<(), DomainError> {
    if !amount.is_positive() {
        return Err(DomainError::NonPositiveAmount(amount));
    }
    if amount.amount() > amount_ceiling().amount() {
        return Err(DomainError::AmountTooLarge(amount));
    }
    Ok(())
}

/// Validates an account's opening balance: either sign (an account may open
/// overdrawn), but no larger in magnitude than [`MAX_AMOUNT_UNITS`]. It seeds
/// every running balance, so it needs the same bound as the amounts summed onto
/// it — without one, the ledger's panic-on-overflow arithmetic was reachable,
/// and release builds abort on panic.
///
/// # Errors
///
/// [`DomainError::OpeningBalanceTooLarge`] if its magnitude exceeds the ceiling.
pub(crate) fn validate_opening_balance(balance: Money) -> Result<(), DomainError> {
    if balance.amount().abs() > amount_ceiling().amount() {
        return Err(DomainError::OpeningBalanceTooLarge(balance));
    }
    Ok(())
}

/// An invariant of a domain type was violated during construction or
/// deserialization.
///
/// Validated types are constructed only through their `new` constructors and
/// deserialize through the same path, so an invalid value (e.g. a month of `13`
/// or a negative amount arriving over IPC) is rejected rather than silently
/// accepted.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum DomainError {
    /// A month component was outside `1..=12`.
    #[error("month {0} is out of range; expected 1..=12")]
    MonthOutOfRange(u8),

    /// A year was outside the supported `1..=9999` range.
    #[error("year {0} is out of range; expected 1..=9999")]
    YearOutOfRange(i32),

    /// An entry amount was zero or negative; amounts are positive magnitudes.
    #[error("entry amount must be a positive magnitude, got {0}")]
    NonPositiveAmount(Money),

    /// An entry amount exceeded the accepted ceiling.
    #[error("entry amount {0} exceeds the maximum accepted value")]
    AmountTooLarge(Money),

    /// An account's opening balance exceeded the accepted ceiling in magnitude.
    #[error("opening balance {0} exceeds the maximum accepted value")]
    OpeningBalanceTooLarge(Money),

    /// A note exceeded [`MAX_NOTE_LEN`].
    #[error("note exceeds {max} characters (was {len})")]
    NoteTooLong {
        /// Actual length in characters.
        len: usize,
        /// The enforced maximum.
        max: usize,
    },

    /// A category label exceeded [`MAX_LABEL_LEN`].
    #[error("label exceeds {max} characters (was {len})")]
    LabelTooLong {
        /// Actual length in characters.
        len: usize,
        /// The enforced maximum.
        max: usize,
    },

    /// A category label (or emoji) was empty.
    #[error("label must not be empty")]
    EmptyLabel,

    /// A currency code was not three ASCII letters (ISO 4217).
    #[error("currency code {0:?} is not a 3-letter ISO 4217 code")]
    InvalidCurrency(String),

    /// A normal account was given member accounts (only summary accounts have
    /// members).
    #[error("a normal account cannot have member accounts")]
    NormalAccountHasMembers,

    /// A summary account was given a non-zero opening balance (its figures are
    /// derived from its members).
    #[error("a summary account cannot have an opening balance")]
    SummaryHasOpeningBalance,

    /// A summary account's member list contained the same account twice.
    #[error("a summary account's members must be distinct")]
    DuplicateMembers,

    /// A summary account had more members than [`MAX_SUMMARY_MEMBERS`].
    #[error("a summary account has {len} members, exceeding the maximum of {max}")]
    TooManyMembers {
        /// Number of members supplied.
        len: usize,
        /// The enforced maximum.
        max: usize,
    },

    /// A recurrence interval was zero; it must be at least one.
    #[error("recurrence interval must be >= 1")]
    ZeroInterval,

    /// A rule's or budget's amount segments were empty, out of order, or not
    /// anchored at its start.
    #[error("amount segments must be non-empty, start at the owner's start, and ascend")]
    InvalidAmountSegments,

    /// A rule carried more amount segments than [`MAX_AMOUNT_SEGMENTS`].
    #[error("amount history has {len} segments, exceeding the maximum of {max}")]
    TooManyAmountSegments {
        /// Number of segments supplied.
        len: usize,
        /// The enforced maximum.
        max: usize,
    },

    /// A recurring rule's `Until` end date preceded its start date.
    #[error("recurrence end date {end} is before its start date {start}")]
    EndBeforeStart {
        /// The rule's start date.
        start: Date,
        /// The offending end date.
        end: Date,
    },

    /// A budget's end month preceded its start month.
    ///
    /// Deliberately separate from [`DomainError::EndBeforeStart`], which is
    /// `Date`-typed and worded for recurrence.
    #[error("budget end month {end} is before its start month {start}")]
    BudgetEndBeforeStart {
        /// The budget's start month.
        start: Month,
        /// The offending end month.
        end: Month,
    },

    /// A monetary value failed to parse.
    #[error(transparent)]
    Money(#[from] MoneyError),
}
