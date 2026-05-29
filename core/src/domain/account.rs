//! Accounts and their currency.
//!
//! Each account tracks income and expenses in a single fixed currency. There is
//! no cross-account conversion or aggregation (see `docs/DESIGN.md` §5).

use serde::{Deserialize, Serialize};

use crate::domain::error::{DomainError, MAX_LABEL_LEN};
use crate::domain::ids::AccountId;
use crate::domain::month::Month;
use crate::money::Money;

/// An ISO 4217 currency code (e.g. `"USD"`). Stores and validates the
/// three-letter code only; display formatting and minor-unit rounding are the
/// frontend's responsibility (`Intl.NumberFormat`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Currency(String);

impl Currency {
    /// Validates and normalizes a currency code to upper case.
    ///
    /// # Errors
    ///
    /// [`DomainError::InvalidCurrency`] unless the trimmed code is exactly three
    /// ASCII letters.
    pub fn new(code: &str) -> Result<Self, DomainError> {
        let upper = code.trim().to_ascii_uppercase();
        if upper.len() == 3 && upper.bytes().all(|b| b.is_ascii_uppercase()) {
            Ok(Self(upper))
        } else {
            // Echo only a bounded snippet so a huge malformed code can't bloat
            // the error value.
            Err(DomainError::InvalidCurrency(
                code.chars().take(64).collect(),
            ))
        }
    }

    /// The three-letter code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Currency {
    type Error = DomainError;

    fn try_from(code: String) -> Result<Self, Self::Error> {
        Self::new(&code)
    }
}

impl From<Currency> for String {
    fn from(currency: Currency) -> Self {
        currency.0
    }
}

/// A tracked account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AccountRepr")]
pub struct Account {
    id: AccountId,
    name: String,
    icon: String,
    currency: Currency,
    opening_balance: Money,
    anchor: Month,
}

#[derive(Deserialize)]
struct AccountRepr {
    id: AccountId,
    name: String,
    icon: String,
    currency: Currency,
    opening_balance: Money,
    anchor: Month,
}

impl TryFrom<AccountRepr> for Account {
    type Error = DomainError;

    fn try_from(repr: AccountRepr) -> Result<Self, Self::Error> {
        Self::new(
            repr.id,
            repr.name,
            repr.icon,
            repr.currency,
            repr.opening_balance,
            repr.anchor,
        )
    }
}

impl Account {
    /// Creates an account.
    ///
    /// `opening_balance` is the balance as of `anchor`; the running-balance
    /// chain starts there (typically `Money::zero()` and the creation month).
    ///
    /// # Errors
    ///
    /// - [`DomainError::EmptyLabel`] if `name` or `icon` is empty.
    /// - [`DomainError::LabelTooLong`] if `name` or `icon` exceeds
    ///   [`MAX_LABEL_LEN`] characters.
    pub fn new(
        id: AccountId,
        name: String,
        icon: String,
        currency: Currency,
        opening_balance: Money,
        anchor: Month,
    ) -> Result<Self, DomainError> {
        let name_len = name.chars().count();
        if name_len == 0 {
            return Err(DomainError::EmptyLabel);
        }
        if name_len > MAX_LABEL_LEN {
            return Err(DomainError::LabelTooLong {
                len: name_len,
                max: MAX_LABEL_LEN,
            });
        }
        let icon_len = icon.chars().count();
        if icon_len == 0 {
            return Err(DomainError::EmptyLabel);
        }
        if icon_len > MAX_LABEL_LEN {
            return Err(DomainError::LabelTooLong {
                len: icon_len,
                max: MAX_LABEL_LEN,
            });
        }
        Ok(Self {
            id,
            name,
            icon,
            currency,
            opening_balance,
            anchor,
        })
    }

    /// Stable identifier.
    #[must_use]
    pub const fn id(&self) -> AccountId {
        self.id
    }

    /// Display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Icon (preset id or emoji).
    #[must_use]
    pub fn icon(&self) -> &str {
        &self.icon
    }

    /// The account's currency.
    #[must_use]
    pub const fn currency(&self) -> &Currency {
        &self.currency
    }

    /// Balance as of [`anchor`](Self::anchor); the chain's starting point.
    #[must_use]
    pub const fn opening_balance(&self) -> Money {
        self.opening_balance
    }

    /// The month from which the running-balance chain starts.
    #[must_use]
    pub const fn anchor(&self) -> Month {
        self.anchor
    }
}

#[cfg(test)]
mod tests {
    use super::{Account, Currency};
    use crate::domain::error::DomainError;
    use crate::domain::ids::AccountId;
    use crate::domain::month::Month;
    use crate::money::Money;

    fn anchor() -> Month {
        Month::new(2026, 5).unwrap()
    }

    #[test]
    fn currency_validation() {
        assert_eq!(Currency::new("usd").unwrap().code(), "USD");
        assert_eq!(Currency::new("  eur ").unwrap().code(), "EUR");
        assert!(matches!(
            Currency::new("US"),
            Err(DomainError::InvalidCurrency(_))
        ));
        assert!(matches!(
            Currency::new("USDD"),
            Err(DomainError::InvalidCurrency(_))
        ));
        assert!(matches!(
            Currency::new("US1"),
            Err(DomainError::InvalidCurrency(_))
        ));
        assert!(matches!(
            Currency::new("€€€"),
            Err(DomainError::InvalidCurrency(_))
        ));
    }

    #[test]
    fn currency_serializes_as_plain_string() {
        let json = serde_json::to_string(&Currency::new("USD").unwrap()).unwrap();
        assert_eq!(json, "\"USD\"");
        assert_eq!(
            serde_json::from_str::<Currency>("\"eur\"").unwrap().code(),
            "EUR"
        );
        assert!(serde_json::from_str::<Currency>("\"nope\"").is_err());
    }

    #[test]
    fn rejects_empty_name() {
        let err = Account::new(
            AccountId::new(1),
            String::new(),
            "💰".to_owned(),
            Currency::new("USD").unwrap(),
            Money::zero(),
            anchor(),
        );
        assert_eq!(err.unwrap_err(), DomainError::EmptyLabel);
    }

    #[test]
    fn rejects_empty_icon() {
        let err = Account::new(
            AccountId::new(1),
            "Checking".to_owned(),
            String::new(),
            Currency::new("USD").unwrap(),
            Money::zero(),
            anchor(),
        );
        assert_eq!(err.unwrap_err(), DomainError::EmptyLabel);
    }

    #[test]
    fn rejects_overlong_name() {
        let err = Account::new(
            AccountId::new(1),
            "x".repeat(201),
            String::new(),
            Currency::new("USD").unwrap(),
            Money::zero(),
            anchor(),
        );
        assert!(matches!(err, Err(DomainError::LabelTooLong { .. })));
    }

    #[test]
    fn round_trips_with_money_as_string() {
        let account = Account::new(
            AccountId::new(7),
            "Checking".to_owned(),
            "🏦".to_owned(),
            Currency::new("USD").unwrap(),
            Money::from_minor_units(10_000, 2),
            anchor(),
        )
        .unwrap();
        let json = serde_json::to_string(&account).unwrap();
        assert!(json.contains(r#""opening_balance":"100.00""#));
        assert!(json.contains(r#""currency":"USD""#));
        assert_eq!(serde_json::from_str::<Account>(&json).unwrap(), account);
    }
}
