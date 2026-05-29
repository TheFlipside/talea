//! Recorded income/expense entries.

use serde::{Deserialize, Serialize};
use time::Date;

use crate::domain::error::{validate_amount, DomainError, MAX_NOTE_LEN};
use crate::domain::ids::{AccountId, CategoryId, EntryId};
use crate::money::Money;

/// Whether an entry adds to or subtracts from the balance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// Money coming in (salary or other).
    Income,
    /// Money going out.
    Expense,
}

impl EntryKind {
    /// Applies this kind's sign to a positive `amount`: unchanged for income,
    /// negated for expense.
    #[must_use]
    pub fn signed(self, amount: Money) -> Money {
        match self {
            Self::Income => amount,
            Self::Expense => -amount,
        }
    }
}

/// A single recorded money movement on an account.
///
/// `amount` is always a **positive magnitude**; the sign is derived from
/// [`kind`](Self::kind) via [`signed_amount`](Self::signed_amount) and never
/// stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "EntryRepr")]
pub struct Entry {
    id: EntryId,
    account_id: AccountId,
    amount: Money,
    kind: EntryKind,
    #[serde(with = "crate::domain::date::iso_date")]
    date: Date,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category_id: Option<CategoryId>,
}

#[derive(Deserialize)]
struct EntryRepr {
    id: EntryId,
    account_id: AccountId,
    amount: Money,
    kind: EntryKind,
    #[serde(with = "crate::domain::date::iso_date")]
    date: Date,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    category_id: Option<CategoryId>,
}

impl TryFrom<EntryRepr> for Entry {
    type Error = DomainError;

    fn try_from(repr: EntryRepr) -> Result<Self, Self::Error> {
        Self::new(
            repr.id,
            repr.account_id,
            repr.amount,
            repr.kind,
            repr.date,
            repr.note,
            repr.category_id,
        )
    }
}

impl Entry {
    /// Creates an entry.
    ///
    /// # Errors
    ///
    /// - [`DomainError::NonPositiveAmount`] / [`DomainError::AmountTooLarge`] if
    ///   `amount` is not a positive magnitude within the accepted ceiling.
    /// - [`DomainError::NoteTooLong`] if `note` exceeds [`MAX_NOTE_LEN`].
    pub fn new(
        id: EntryId,
        account_id: AccountId,
        amount: Money,
        kind: EntryKind,
        date: Date,
        note: Option<String>,
        category_id: Option<CategoryId>,
    ) -> Result<Self, DomainError> {
        validate_amount(amount)?;
        if let Some(ref text) = note {
            let len = text.chars().count();
            if len > MAX_NOTE_LEN {
                return Err(DomainError::NoteTooLong {
                    len,
                    max: MAX_NOTE_LEN,
                });
            }
        }
        Ok(Self {
            id,
            account_id,
            amount,
            kind,
            date,
            note,
            category_id,
        })
    }

    /// Stable identifier.
    #[must_use]
    pub const fn id(&self) -> EntryId {
        self.id
    }

    /// The account this entry belongs to.
    #[must_use]
    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }

    /// The positive magnitude of the entry.
    #[must_use]
    pub const fn amount(&self) -> Money {
        self.amount
    }

    /// Whether the entry is income or expense.
    #[must_use]
    pub const fn kind(&self) -> EntryKind {
        self.kind
    }

    /// The date the entry is recorded on.
    #[must_use]
    pub const fn date(&self) -> Date {
        self.date
    }

    /// Free-text memo, if any.
    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// The category this entry is classified under, if any.
    #[must_use]
    pub const fn category_id(&self) -> Option<CategoryId> {
        self.category_id
    }

    /// The signed contribution to the balance (`+amount` income, `-amount`
    /// expense).
    #[must_use]
    pub fn signed_amount(&self) -> Money {
        self.kind.signed(self.amount)
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, EntryKind};
    use crate::domain::error::DomainError;
    use crate::domain::ids::{AccountId, EntryId};
    use crate::money::Money;
    use time::{Date, Month};

    fn date() -> Date {
        Date::from_calendar_date(2026, Month::May, 9).unwrap()
    }

    #[test]
    fn signed_amount_follows_kind() {
        let income = Entry::new(
            EntryId::new(1),
            AccountId::new(1),
            Money::from_minor_units(5000, 2),
            EntryKind::Income,
            date(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(income.signed_amount(), Money::from_minor_units(5000, 2));

        let expense = Entry::new(
            EntryId::new(2),
            AccountId::new(1),
            Money::from_minor_units(5000, 2),
            EntryKind::Expense,
            date(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(expense.signed_amount(), Money::from_minor_units(-5000, 2));
    }

    #[test]
    fn rejects_non_positive_amount() {
        for bad in [Money::zero(), Money::from_minor_units(-1, 2)] {
            let err = Entry::new(
                EntryId::new(1),
                AccountId::new(1),
                bad,
                EntryKind::Expense,
                date(),
                None,
                None,
            );
            assert!(matches!(err, Err(DomainError::NonPositiveAmount(_))));
        }
    }

    #[test]
    fn rejects_amount_over_ceiling() {
        // 2 quadrillion in major units, above the 1-quadrillion ceiling.
        let huge = Money::from_minor_units(2_000_000_000_000_000, 0);
        let err = Entry::new(
            EntryId::new(1),
            AccountId::new(1),
            huge,
            EntryKind::Income,
            date(),
            None,
            None,
        );
        assert!(matches!(err, Err(DomainError::AmountTooLarge(_))));
    }

    #[test]
    fn rejects_overlong_note() {
        let err = Entry::new(
            EntryId::new(1),
            AccountId::new(1),
            Money::from_minor_units(100, 2),
            EntryKind::Expense,
            date(),
            Some("x".repeat(1001)),
            None,
        );
        assert!(matches!(err, Err(DomainError::NoteTooLong { .. })));
    }

    #[test]
    fn serde_round_trip_with_string_amount_and_iso_date() {
        let entry = Entry::new(
            EntryId::new(9),
            AccountId::new(2),
            Money::from_minor_units(1299, 2),
            EntryKind::Expense,
            date(),
            Some("Coffee".to_owned()),
            None,
        )
        .unwrap();
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""amount":"12.99""#));
        assert!(json.contains(r#""date":"2026-05-09""#));
        assert!(json.contains(r#""kind":"expense""#));
        assert_eq!(serde_json::from_str::<Entry>(&json).unwrap(), entry);
    }

    #[test]
    fn deserialize_rejects_invalid_values() {
        // Negative amount over the wire must be rejected.
        let bad_amount =
            r#"{"id":1,"account_id":1,"amount":"-5","kind":"income","date":"2026-05-09"}"#;
        assert!(serde_json::from_str::<Entry>(bad_amount).is_err());
        // Bare-number amount (float) must be rejected.
        let float_amount =
            r#"{"id":1,"account_id":1,"amount":5.0,"kind":"income","date":"2026-05-09"}"#;
        assert!(serde_json::from_str::<Entry>(float_amount).is_err());
    }
}
