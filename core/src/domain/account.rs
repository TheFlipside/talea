//! Accounts and their currency.
//!
//! Each account tracks income and expenses in a single fixed currency. There is
//! no cross-account conversion, and the only aggregation is a summary account's,
//! over members that share its currency (see `docs/DESIGN.md` §5 and §11).

use serde::{Deserialize, Serialize};

use crate::domain::error::{
    validate_opening_balance, DomainError, MAX_LABEL_LEN, MAX_SUMMARY_MEMBERS, MIN_ANCHOR_YEAR,
};
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
        Self::from_parts(
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
    /// - [`DomainError::OpeningBalanceTooLarge`] if `opening_balance` exceeds
    ///   one quadrillion in magnitude, the ceiling amounts share.
    /// - [`DomainError::AnchorTooEarly`] if `anchor` is before
    ///   [`MIN_ANCHOR_YEAR`].
    pub fn new(
        id: AccountId,
        name: String,
        icon: String,
        currency: Currency,
        opening_balance: Money,
        anchor: Month,
    ) -> Result<Self, DomainError> {
        Self::from_parts(
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

    /// Creates a summary account aggregating `members`. A summary holds no
    /// balance of its own, so its opening balance is fixed to zero; its figures
    /// are derived from its members. The rules about the members themselves
    /// (that there is one, and each exists, is normal and shares `currency`)
    /// need the other accounts, so they are [`Account::check_members`]'s.
    ///
    /// # Errors
    ///
    /// - [`DomainError::EmptyLabel`] / [`DomainError::LabelTooLong`] /
    ///   [`DomainError::AnchorTooEarly`] as for [`Account::new`].
    /// - [`DomainError::DuplicateMembers`] if `members` repeats an id.
    /// - [`DomainError::TooManyMembers`] if there are more than
    ///   [`MAX_SUMMARY_MEMBERS`].
    pub fn new_summary(
        id: AccountId,
        name: String,
        icon: String,
        currency: Currency,
        anchor: Month,
        members: Vec<AccountId>,
    ) -> Result<Self, DomainError> {
        Self::from_parts(
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

    /// Rebuilds an account of either kind from every one of its parts, checking
    /// all that [`Account::new`] and [`Account::new_summary`] check plus the two
    /// rules neither can be handed a way to break: a normal account has no
    /// members, and a summary no opening balance. For loading stored rows, which
    /// must be validated rather than repaired.
    ///
    /// # Errors
    ///
    /// Any error of [`Account::new`] or [`Account::new_summary`], or
    /// [`DomainError::NormalAccountHasMembers`] /
    /// [`DomainError::SummaryHasOpeningBalance`].
    // One argument per stored column: bundling them would only move the list.
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
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
        if anchor.year() < MIN_ANCHOR_YEAR {
            return Err(DomainError::AnchorTooEarly);
        }
        match kind {
            AccountKind::Normal => {
                if !members.is_empty() {
                    return Err(DomainError::NormalAccountHasMembers);
                }
                validate_opening_balance(opening_balance)?;
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
                // same currency, no nesting — need the other accounts, so they
                // live in `check_members`).
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

    /// Checks a summary account's members against the accounts they name: the
    /// cross-account rules the constructors can't see, since each member is a
    /// separate account. `lookup` resolves a member id to its account, or to
    /// `None` if there is none. A summary needs at least one member, and each
    /// must exist, be a normal account (no nesting), share this account's
    /// currency, and not be this account. A normal account has no members, so
    /// it always passes.
    ///
    /// # Errors
    /// - [`DomainError::SummaryWithoutMembers`] if a summary has no members.
    /// - [`DomainError::SummaryIncludesItself`] if a member is this account.
    /// - [`DomainError::MissingMember`] if `lookup` finds no account for a member.
    /// - [`DomainError::NestedSummary`] if a member is itself a summary.
    /// - [`DomainError::MemberCurrencyMismatch`] if a member's currency differs.
    pub fn check_members<'a>(
        &self,
        lookup: impl Fn(AccountId) -> Option<&'a Self>,
    ) -> Result<(), DomainError> {
        // Exhaustive, so a new kind of account has to decide its own rules
        // rather than pass by default.
        match self.kind {
            AccountKind::Normal => Ok(()),
            AccountKind::Summary => self.check_summary_members(lookup),
        }
    }

    /// [`Account::check_members`] for a summary account.
    fn check_summary_members<'a>(
        &self,
        lookup: impl Fn(AccountId) -> Option<&'a Self>,
    ) -> Result<(), DomainError> {
        if self.members.is_empty() {
            return Err(DomainError::SummaryWithoutMembers);
        }
        for &member_id in &self.members {
            if member_id == self.id {
                return Err(DomainError::SummaryIncludesItself);
            }
            // An account found under another id is as good as missing.
            let member = lookup(member_id)
                .filter(|member| member.id == member_id)
                .ok_or(DomainError::MissingMember)?;
            if member.kind != AccountKind::Normal {
                return Err(DomainError::NestedSummary);
            }
            if member.currency != self.currency {
                return Err(DomainError::MemberCurrencyMismatch);
            }
        }
        Ok(())
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
        assert_eq!(normal.members(), []);
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
    fn caps_the_opening_balance_in_either_direction() {
        let opening = |minor: i64| {
            Account::new(
                AccountId::new(1),
                "Checking".to_owned(),
                "🏦".to_owned(),
                Currency::new("USD").unwrap(),
                Money::from_minor_units(minor, 2),
                anchor(),
            )
        };
        // One quadrillion is the ceiling, overdrawn or not.
        let cap = 100_000_000_000_000_000; // 1e15 at two decimal places
        assert!(opening(cap).is_ok());
        assert!(opening(-cap).is_ok());
        for beyond in [cap + 1, -cap - 1] {
            assert!(matches!(
                opening(beyond),
                Err(DomainError::OpeningBalanceTooLarge(_))
            ));
        }
        // Deserialization — the IPC and database-load path — enforces it too,
        // at both of `Decimal`'s extremes.
        for extreme in [
            "79228162514264337593543950335",
            "-79228162514264337593543950335",
        ] {
            let json = format!(
                r#"{{"id":1,"name":"Checking","icon":"🏦","currency":"USD",
                "opening_balance":"{extreme}","anchor":{{"year":2026,"month":5}},
                "kind":"normal","members":[]}}"#
            );
            assert!(serde_json::from_str::<Account>(&json).is_err());
        }
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

    fn normal(id: u64, currency: &str) -> Account {
        Account::new(
            AccountId::new(id),
            "Checking".to_owned(),
            "🏦".to_owned(),
            Currency::new(currency).unwrap(),
            Money::zero(),
            anchor(),
        )
        .unwrap()
    }

    /// Checks `account`'s members against `accounts`, as a caller would after
    /// loading them.
    fn check(account: &Account, accounts: &[Account]) -> Result<(), DomainError> {
        account.check_members(|id| accounts.iter().find(|other| other.id() == id))
    }

    #[test]
    fn accepts_a_summary_over_same_currency_normal_accounts() {
        let all = summary(vec![AccountId::new(2), AccountId::new(3)]).unwrap();
        assert_eq!(check(&all, &[normal(2, "USD"), normal(3, "USD")]), Ok(()));
        // A normal account has no members to check, whatever else exists.
        assert_eq!(check(&normal(2, "USD"), &[]), Ok(()));
    }

    #[test]
    fn rejects_each_broken_membership_rule() {
        // Each case breaks exactly one rule, and the member at fault comes
        // second, so a check that only looked at the first member would pass.
        let cases = [
            (vec![], vec![], DomainError::SummaryWithoutMembers),
            (
                vec![2, 1],
                vec![normal(2, "USD")],
                DomainError::SummaryIncludesItself,
            ),
            (
                vec![2, 3],
                vec![normal(2, "USD")],
                DomainError::MissingMember,
            ),
            (
                vec![2, 3],
                vec![
                    normal(2, "USD"),
                    Account::new_summary(
                        AccountId::new(3),
                        "Nested".to_owned(),
                        "📊".to_owned(),
                        Currency::new("USD").unwrap(),
                        anchor(),
                        vec![AccountId::new(2)],
                    )
                    .unwrap(),
                ],
                DomainError::NestedSummary,
            ),
            (
                vec![2, 3],
                vec![normal(2, "USD"), normal(3, "EUR")],
                DomainError::MemberCurrencyMismatch,
            ),
        ];
        for (members, accounts, expected) in cases {
            let all = summary(members.into_iter().map(AccountId::new).collect()).unwrap();
            assert_eq!(check(&all, &accounts), Err(expected));
        }
    }

    #[test]
    fn a_lookup_that_answers_with_another_account_counts_as_missing() {
        let all = summary(vec![AccountId::new(2)]).unwrap();
        let other = normal(3, "USD");
        assert_eq!(
            all.check_members(|_| Some(&other)),
            Err(DomainError::MissingMember)
        );
    }

    #[test]
    fn account_errors_keep_their_wording() {
        // The command layer shows these to the user as they are.
        for (error, text) in [
            (
                DomainError::AnchorTooEarly,
                "an account cannot start before 1969",
            ),
            (
                DomainError::SummaryWithoutMembers,
                "a summary account needs at least one member account",
            ),
            (
                DomainError::SummaryIncludesItself,
                "a summary account cannot include itself",
            ),
            (
                DomainError::MissingMember,
                "a member account no longer exists",
            ),
            (
                DomainError::NestedSummary,
                "a summary account's members must be normal accounts",
            ),
            (
                DomainError::MemberCurrencyMismatch,
                "all member accounts must share the summary account's currency",
            ),
        ] {
            assert_eq!(error.to_string(), text);
        }
    }

    /// Builds an account through `from_parts` with otherwise valid parts.
    fn from_parts(
        kind: AccountKind,
        opening: Money,
        members: Vec<AccountId>,
    ) -> Result<Account, DomainError> {
        Account::from_parts(
            AccountId::new(1),
            "Checking".to_owned(),
            "🏦".to_owned(),
            Currency::new("USD").unwrap(),
            opening,
            anchor(),
            kind,
            members,
        )
    }

    #[test]
    fn from_parts_refuses_what_the_typed_constructors_cannot_be_handed() {
        let some = Money::from_minor_units(12_345, 2);
        assert!(from_parts(AccountKind::Normal, some, vec![]).is_ok());
        assert!(from_parts(AccountKind::Summary, Money::zero(), vec![AccountId::new(2)]).is_ok());
        assert_eq!(
            from_parts(AccountKind::Normal, some, vec![AccountId::new(2)]),
            Err(DomainError::NormalAccountHasMembers)
        );
        assert_eq!(
            from_parts(AccountKind::Summary, some, vec![AccountId::new(2)]),
            Err(DomainError::SummaryHasOpeningBalance)
        );
    }

    #[test]
    fn refuses_an_anchor_before_1969() {
        let starting = |year: i32, month: u8| {
            Account::new(
                AccountId::new(1),
                "Checking".to_owned(),
                "🏦".to_owned(),
                Currency::new("USD").unwrap(),
                Money::zero(),
                Month::new(year, month).unwrap(),
            )
        };
        // A device clock at the Unix epoch, west of Greenwich.
        assert!(starting(1969, 12).is_ok());
        assert_eq!(starting(1968, 12), Err(DomainError::AnchorTooEarly));
        assert_eq!(starting(1, 1), Err(DomainError::AnchorTooEarly));
        // Summaries, and the IPC path, keep to it too.
        assert_eq!(
            Account::new_summary(
                AccountId::new(1),
                "All".to_owned(),
                "📊".to_owned(),
                Currency::new("USD").unwrap(),
                Month::new(1968, 12).unwrap(),
                vec![AccountId::new(2)],
            ),
            Err(DomainError::AnchorTooEarly)
        );
        let json = r#"{"id":1,"name":"Checking","icon":"🏦","currency":"USD",
            "opening_balance":"0","anchor":{"year":1968,"month":12},
            "kind":"normal","members":[]}"#;
        assert!(serde_json::from_str::<Account>(json).is_err());
    }
}
