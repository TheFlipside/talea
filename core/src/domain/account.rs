//! Accounts and their currency.
//!
//! Each account tracks income and expenses in a single fixed currency. There is
//! no cross-account conversion or aggregation (see `docs/DESIGN.md` §5).

use serde::{Deserialize, Serialize};

use crate::domain::error::{DomainError, MAX_LABEL_LEN, MAX_SUMMARY_MEMBERS};
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

/// Which kind of account this is.
///
/// A [`AccountKind::Normal`] account records its own income/expenses. A
/// [`AccountKind::Summary`] account records nothing itself — it is a read-only
/// overview that aggregates several same-currency normal accounts (its
/// `members`). See `docs/DESIGN.md` §11.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    /// An ordinary account that holds entries and recurring rules.
    #[default]
    Normal,
    /// A read-only account aggregating its member accounts' figures.
    Summary,
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
    kind: AccountKind,
    /// Member account ids — non-empty only for [`AccountKind::Summary`].
    members: Vec<AccountId>,
}

#[derive(Deserialize)]
struct AccountRepr {
    id: AccountId,
    name: String,
    icon: String,
    currency: Currency,
    opening_balance: Money,
    anchor: Month,
    // Defaulted so accounts persisted/sent before summary accounts existed still
    // deserialize as ordinary accounts.
    #[serde(default)]
    kind: AccountKind,
    #[serde(default)]
    members: Vec<AccountId>,
}

impl TryFrom<AccountRepr> for Account {
    type Error = DomainError;

    fn try_from(repr: AccountRepr) -> Result<Self, Self::Error> {
        Self::build(
            repr.id,
            repr.name,
            repr.icon,
            repr.currency,
            repr.opening_balance,
            repr.anchor,
            repr.kind,
            repr.members,
        )
    }
}

impl Account {
    /// Creates a normal account.
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
        Self::build(
            id,
            name,
            icon,
            currency,
            opening_balance,
            anchor,
            AccountKind::Normal,
            Vec::new(),
        )
    }

    /// Creates a summary account aggregating `members` (which must all share
    /// `currency` — enforced by the caller, since `core` can't see other
    /// accounts). A summary holds no balance of its own, so its opening balance
    /// is fixed to zero; its figures are derived from its members.
    ///
    /// # Errors
    ///
    /// - [`DomainError::EmptyLabel`] / [`DomainError::LabelTooLong`] as for
    ///   [`Account::new`].
    /// - [`DomainError::DuplicateMembers`] if `members` repeats an id.
    pub fn new_summary(
        id: AccountId,
        name: String,
        icon: String,
        currency: Currency,
        anchor: Month,
        members: Vec<AccountId>,
    ) -> Result<Self, DomainError> {
        Self::build(
            id,
            name,
            icon,
            currency,
            Money::zero(),
            anchor,
            AccountKind::Summary,
            members,
        )
    }

    /// Shared constructor/validator for both account kinds.
    #[allow(clippy::too_many_arguments)]
    fn build(
        id: AccountId,
        name: String,
        icon: String,
        currency: Currency,
        opening_balance: Money,
        anchor: Month,
        kind: AccountKind,
        members: Vec<AccountId>,
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
        match kind {
            AccountKind::Normal => {
                if !members.is_empty() {
                    return Err(DomainError::NormalAccountHasMembers);
                }
            }
            AccountKind::Summary => {
                if opening_balance != Money::zero() {
                    return Err(DomainError::SummaryHasOpeningBalance);
                }
                if members.len() > MAX_SUMMARY_MEMBERS {
                    return Err(DomainError::TooManyMembers {
                        len: members.len(),
                        max: MAX_SUMMARY_MEMBERS,
                    });
                }
                // Reject duplicate members (cross-account checks — existence,
                // same currency, no nesting — live in the command layer).
                let mut seen = members.clone();
                seen.sort_unstable();
                seen.dedup();
                if seen.len() != members.len() {
                    return Err(DomainError::DuplicateMembers);
                }
            }
        }
        Ok(Self {
            id,
            name,
            icon,
            currency,
            opening_balance,
            anchor,
            kind,
            members,
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

    /// Whether this is a normal or a summary account.
    #[must_use]
    pub const fn kind(&self) -> AccountKind {
        self.kind
    }

    /// The member account ids — non-empty only for a summary account.
    #[must_use]
    pub fn members(&self) -> &[AccountId] {
        &self.members
    }
}

#[cfg(test)]
mod tests {
    use super::{Account, AccountKind, Currency};
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

    fn summary(members: Vec<AccountId>) -> Result<Account, DomainError> {
        Account::new_summary(
            AccountId::new(1),
            "All accounts".to_owned(),
            "📊".to_owned(),
            Currency::new("USD").unwrap(),
            anchor(),
            members,
        )
    }

    #[test]
    fn summary_account_round_trips_with_members() {
        let account = summary(vec![AccountId::new(2), AccountId::new(3)]).unwrap();
        assert_eq!(account.kind(), AccountKind::Summary);
        assert_eq!(account.members(), &[AccountId::new(2), AccountId::new(3)]);
        // A summary holds no balance of its own.
        assert_eq!(account.opening_balance(), Money::zero());
        let json = serde_json::to_string(&account).unwrap();
        assert!(json.contains(r#""kind":"summary""#));
        assert_eq!(serde_json::from_str::<Account>(&json).unwrap(), account);
    }

    #[test]
    fn normal_account_has_no_members() {
        let normal = Account::new(
            AccountId::new(1),
            "Checking".to_owned(),
            "🏦".to_owned(),
            Currency::new("USD").unwrap(),
            Money::zero(),
            anchor(),
        )
        .unwrap();
        assert_eq!(normal.kind(), AccountKind::Normal);
        assert!(normal.members().is_empty());
    }

    #[test]
    fn rejects_normal_account_with_members() {
        // Only constructible via deserialization (the typed `new` can't express it).
        let json = r#"{"id":1,"name":"Checking","icon":"🏦","currency":"USD",
            "opening_balance":"0","anchor":{"year":2026,"month":5},
            "kind":"normal","members":[2]}"#;
        assert!(serde_json::from_str::<Account>(json).is_err());
    }

    #[test]
    fn rejects_summary_with_opening_balance() {
        let json = r#"{"id":1,"name":"All","icon":"📊","currency":"USD",
            "opening_balance":"5.00","anchor":{"year":2026,"month":5},
            "kind":"summary","members":[2]}"#;
        assert!(serde_json::from_str::<Account>(json).is_err());
    }

    #[test]
    fn rejects_duplicate_members() {
        let err = summary(vec![AccountId::new(2), AccountId::new(2)]).unwrap_err();
        assert_eq!(err, DomainError::DuplicateMembers);
    }

    #[test]
    fn rejects_too_many_members() {
        let many: Vec<AccountId> = (2..=100).map(AccountId::new).collect();
        assert!(matches!(
            summary(many),
            Err(DomainError::TooManyMembers { .. })
        ));
    }
}
