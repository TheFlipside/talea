//! Wire input types for create commands.
//!
//! Entities are created with database-assigned ids, so the create payloads omit
//! the id. Each `build` constructs the corresponding domain type (with a
//! placeholder id) through its validating constructor, so invalid input is
//! rejected before any database write.

use serde::Deserialize;
use talea_core::{
    Account, AccountId, AccountKind, Category, CategoryIcon, CategoryId, Currency, DomainError,
    Entry, EntryId, EntryKind, Frequency, Money, Month, RecurringRule, RecurringRuleId, RuleEnd,
};
use time::Date;

/// Identifies a single occurrence of a recurring rule (the rule plus the
/// occurrence's date), for the skip / detach commands.
#[derive(Debug, Deserialize)]
pub struct OccurrenceRef {
    pub rule_id: RecurringRuleId,
    #[serde(with = "iso_date")]
    pub date: Date,
}

// Dates arrive from the frontend as ISO `YYYY-MM-DD` strings (the core's own
// date serde module is crate-private and unreachable here).
time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

/// Placeholder id for a not-yet-persisted draft; replaced by the real rowid.
const DRAFT_ID: u64 = 0;

/// Create payload for an [`Account`].
///
/// `kind`/`members` default for older clients: an omitted `kind` is a normal
/// account. For a summary account, `members` lists the aggregated accounts
/// (cross-account validation happens in the command layer).
#[derive(Debug, Deserialize)]
pub struct NewAccount {
    pub name: String,
    pub icon: String,
    pub currency: Currency,
    pub opening_balance: Money,
    pub anchor: Month,
    #[serde(default)]
    pub kind: AccountKind,
    #[serde(default)]
    pub members: Vec<AccountId>,
}

impl NewAccount {
    /// Builds a validated draft account (with a placeholder id).
    ///
    /// # Errors
    /// [`DomainError`] if any field is invalid.
    pub fn build(self) -> Result<Account, DomainError> {
        match self.kind {
            AccountKind::Normal => Account::new(
                AccountId::new(DRAFT_ID),
                self.name,
                self.icon,
                self.currency,
                self.opening_balance,
                self.anchor,
            ),
            AccountKind::Summary => Account::new_summary(
                AccountId::new(DRAFT_ID),
                self.name,
                self.icon,
                self.currency,
                self.anchor,
                self.members,
            ),
        }
    }
}

/// Create payload for a [`Category`].
#[derive(Debug, Deserialize)]
pub struct NewCategory {
    pub label: String,
    pub icon: CategoryIcon,
}

impl NewCategory {
    /// # Errors
    /// [`DomainError`] if the label or icon is invalid.
    pub fn build(self) -> Result<Category, DomainError> {
        Category::new(CategoryId::new(DRAFT_ID), self.label, self.icon)
    }
}

/// Create payload for an [`Entry`].
#[derive(Debug, Deserialize)]
pub struct NewEntry {
    pub account_id: AccountId,
    pub amount: Money,
    pub kind: EntryKind,
    #[serde(with = "iso_date")]
    pub date: Date,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub category_id: Option<CategoryId>,
}

impl NewEntry {
    /// # Errors
    /// [`DomainError`] if the amount or note is invalid.
    pub fn build(self) -> Result<Entry, DomainError> {
        Entry::new(
            EntryId::new(DRAFT_ID),
            self.account_id,
            self.amount,
            self.kind,
            self.date,
            self.note,
            self.category_id,
        )
    }
}

/// Create payload for a [`RecurringRule`].
#[derive(Debug, Deserialize)]
pub struct NewRule {
    pub account_id: AccountId,
    pub amount: Money,
    pub kind: EntryKind,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub category_id: Option<CategoryId>,
    #[serde(with = "iso_date")]
    pub start_date: Date,
    pub end: RuleEnd,
    pub frequency: Frequency,
}

impl NewRule {
    /// # Errors
    /// [`DomainError`] if the amount, note, or end/start ordering is invalid.
    pub fn build(self) -> Result<RecurringRule, DomainError> {
        RecurringRule::new(
            RecurringRuleId::new(DRAFT_ID),
            self.account_id,
            self.amount,
            self.kind,
            self.note,
            self.category_id,
            self.start_date,
            self.end,
            self.frequency,
        )
    }
}
