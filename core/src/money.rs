//! Exact monetary values for Talea.
//!
//! Money is represented as a newtype over [`rust_decimal::Decimal`], giving
//! exact base-10 arithmetic. **Floating point is never used for money** — not in
//! storage, not in arithmetic, and not across the frontend boundary, where
//! [`Money`] (de)serializes as a string rather than a JSON number.

use std::fmt;
use std::ops::{Add, Neg, Sub};
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::{Serialize, Serializer};
use thiserror::Error;

/// Errors that can arise when constructing or parsing [`Money`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MoneyError {
    /// The provided text was not a valid decimal amount.
    #[error("invalid money amount: {0}")]
    Parse(String),
}

/// An exact monetary amount.
///
/// Backed by [`Decimal`], so values such as `0.10` are represented precisely.
/// The amount is currency-agnostic; currency is tracked elsewhere in the domain
/// (see [`crate::domain`]).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Money(Decimal);

impl Money {
    /// The zero amount.
    #[must_use]
    pub const fn zero() -> Self {
        Self(Decimal::ZERO)
    }

    /// Wraps a [`Decimal`] as a [`Money`].
    #[must_use]
    pub const fn from_decimal(amount: Decimal) -> Self {
        Self(amount)
    }

    /// Builds a [`Money`] from an integer number of minor units at the given
    /// scale — e.g. `from_minor_units(1234, 2)` is `12.34`.
    #[must_use]
    pub fn from_minor_units(units: i64, scale: u32) -> Self {
        Self(Decimal::new(units, scale))
    }

    /// Parses a decimal amount from text, e.g. `"12.34"` or `"-0.01"`.
    ///
    /// # Errors
    ///
    /// Returns [`MoneyError::Parse`] if `text` is not a valid decimal amount.
    pub fn try_from_str(text: &str) -> Result<Self, MoneyError> {
        Decimal::from_str(text.trim()).map(Self).map_err(|_| {
            // Echo only a bounded snippet: a malformed value arriving over IPC
            // could otherwise be arbitrarily large and bloat the error.
            MoneyError::Parse(text.chars().take(64).collect())
        })
    }

    /// The underlying [`Decimal`] amount.
    #[must_use]
    pub const fn amount(self) -> Decimal {
        self.0
    }

    /// Returns `true` if the amount is exactly zero.
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.0.is_zero()
    }

    /// Returns `true` if the amount is strictly less than zero.
    #[must_use]
    pub fn is_negative(self) -> bool {
        self.0.is_sign_negative() && !self.0.is_zero()
    }

    /// Returns `true` if the amount is strictly greater than zero.
    #[must_use]
    pub fn is_positive(self) -> bool {
        self.0.is_sign_positive() && !self.0.is_zero()
    }

    /// Checked addition; returns `None` on overflow.
    #[must_use]
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }

    /// Checked subtraction; returns `None` on overflow.
    #[must_use]
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(Self)
    }

    /// Returns the amount rounded to `dp` decimal places (banker's rounding),
    /// e.g. for display in a chosen currency's minor units.
    #[must_use]
    pub fn round_dp(self, dp: u32) -> Self {
        Self(self.0.round_dp(dp))
    }
}

impl From<Decimal> for Money {
    fn from(amount: Decimal) -> Self {
        Self(amount)
    }
}

impl FromStr for Money {
    type Err = MoneyError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(text)
    }
}

/// Adds two amounts.
///
/// Like [`Decimal`]'s own `+`, this **panics on overflow** (it does not wrap or
/// saturate). Use [`Money::checked_add`] where overflow must be handled
/// gracefully, e.g. when summing untrusted input.
impl Add for Money {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

/// Subtracts two amounts.
///
/// Like [`Decimal`]'s own `-`, this **panics on overflow**. Use
/// [`Money::checked_sub`] where overflow must be handled gracefully.
impl Sub for Money {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Neg for Money {
    type Output = Self;

    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Money({})", self.0)
    }
}

// Money crosses the frontend boundary as a *string*, never a JSON number, to
// keep floating point entirely out of the wire format.
impl Serialize for Money {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MoneyVisitor;

        impl Visitor<'_> for MoneyVisitor {
            type Value = Money;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a decimal amount encoded as a string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Money, E>
            where
                E: de::Error,
            {
                Money::try_from_str(value).map_err(|e| de::Error::custom(e.to_string()))
            }
        }

        deserializer.deserialize_str(MoneyVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::{Money, MoneyError};
    use rust_decimal_macros::dec;

    #[test]
    fn parses_and_formats_round_trip() {
        let m = Money::try_from_str("12.34").unwrap();
        assert_eq!(m.amount(), dec!(12.34));
        assert_eq!(m.to_string(), "12.34");
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(
            Money::try_from_str("not money"),
            Err(MoneyError::Parse("not money".to_owned()))
        );
    }

    #[test]
    fn from_minor_units_is_exact() {
        assert_eq!(Money::from_minor_units(1234, 2).amount(), dec!(12.34));
        assert_eq!(Money::from_minor_units(-1, 2).amount(), dec!(-0.01));
    }

    #[test]
    fn arithmetic_is_exact_not_floating_point() {
        // The classic 0.1 + 0.2 != 0.3 float trap must not happen here.
        let sum = Money::try_from_str("0.1").unwrap() + Money::try_from_str("0.2").unwrap();
        assert_eq!(sum, Money::try_from_str("0.3").unwrap());
    }

    #[test]
    fn sign_predicates() {
        assert!(Money::zero().is_zero());
        assert!(Money::from_minor_units(-5, 2).is_negative());
        assert!(Money::from_minor_units(5, 2).is_positive());
        assert!(!Money::zero().is_positive());
        assert!(!Money::zero().is_negative());
    }

    #[test]
    fn checked_arithmetic_handles_overflow() {
        let max = Money::from_decimal(rust_decimal::Decimal::MAX);
        assert!(max.checked_add(Money::from_minor_units(1, 0)).is_none());
        assert_eq!(
            Money::from_minor_units(300, 2).checked_sub(Money::from_minor_units(100, 2)),
            Some(Money::from_minor_units(200, 2))
        );
    }

    #[test]
    fn serializes_as_string_not_number() {
        let json = serde_json::to_string(&Money::try_from_str("9.99").unwrap()).unwrap();
        assert_eq!(json, "\"9.99\"");

        let back: Money = serde_json::from_str("\"9.99\"").unwrap();
        assert_eq!(back, Money::try_from_str("9.99").unwrap());
    }

    #[test]
    fn rejects_numeric_json_to_keep_floats_out() {
        // A bare number must be rejected: money never crosses as a float.
        assert!(serde_json::from_str::<Money>("9.99").is_err());
    }

    #[test]
    fn round_dp_uses_bankers_rounding() {
        // Both cases discriminate banker's (half-to-even) from half-up:
        // 2.345 -> 2.34 (down to even 4); half-up would give 2.35.
        assert_eq!(
            Money::try_from_str("2.345").unwrap().round_dp(2),
            Money::try_from_str("2.34").unwrap()
        );
        // 2.355 -> 2.36 (up to even 6); half-up would also give 2.36, but the
        // pair together pins the round-half-to-even behavior.
        assert_eq!(
            Money::try_from_str("2.355").unwrap().round_dp(2),
            Money::try_from_str("2.36").unwrap()
        );
    }
}
