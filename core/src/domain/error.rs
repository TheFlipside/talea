//! Domain-level validation errors and the limits they enforce.

use thiserror::Error;
use time::Date;

use crate::money::{Money, MoneyError};

/// Maximum length (in characters) of a free-text note on an entry or rule.
pub const MAX_NOTE_LEN: usize = 1_000;

/// Maximum length (in characters) of a category label.
pub const MAX_LABEL_LEN: usize = 200;

/// Largest single amount accepted (one quadrillion). Far below
/// `Decimal::MAX` (~7.9e28), so even pathological prefix-sums over the bounded
/// month range cannot overflow the ledger's arithmetic.
const MAX_AMOUNT_MINOR: i64 = 1_000_000_000_000_000;

/// Validates an entry/rule amount: a positive magnitude within
/// [`MAX_AMOUNT_MINOR`].
///
/// # Errors
///
/// [`DomainError::NonPositiveAmount`] if zero/negative;
/// [`DomainError::AmountTooLarge`] if it exceeds the ceiling.
pub(crate) fn validate_amount(amount: Money) -> Result<(), DomainError> {
    if !amount.is_positive() {
        return Err(DomainError::NonPositiveAmount(amount));
    }
    if amount.amount() > Money::from_minor_units(MAX_AMOUNT_MINOR, 0).amount() {
        return Err(DomainError::AmountTooLarge(amount));
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

    /// A recurrence interval was zero; it must be at least one.
    #[error("recurrence interval must be >= 1")]
    ZeroInterval,

    /// A recurring rule's `Until` end date preceded its start date.
    #[error("recurrence end date {end} is before its start date {start}")]
    EndBeforeStart {
        /// The rule's start date.
        start: Date,
        /// The offending end date.
        end: Date,
    },

    /// A monetary value failed to parse.
    #[error(transparent)]
    Money(#[from] MoneyError),
}
