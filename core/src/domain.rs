//! Talea domain model — **STUBBED, INTENTIONALLY INCOMPLETE.**
//!
//! ⚠️ DESIGN DECISION PENDING — DO NOT FINALIZE THIS MODEL OR THE SQLITE SCHEMA.
//!
//! The relationships between the four core entities below — [`Month`],
//! [`Category`], [`Budget`], and [`Transaction`] — are deliberately **not**
//! wired up yet, because they depend on a budgeting-paradigm decision that has
//! not been made:
//!
//! - **Envelope** (zero-based, money allocated into per-category envelopes that
//!   draw down and may carry over), vs.
//! - **Flexible** (per-category targets/limits compared against spending), vs.
//! - a **hybrid** of the two.
//!
//! That choice determines what a [`Budget`] *means*, whether balances carry over
//! between months, and ultimately the `SQLite` schema. See `docs/DESIGN.md` §1.
//!
//! Until it is decided, the types here are minimal placeholders carrying only
//! fields that are safe regardless of the paradigm. Treat every `TODO` and
//! `DESIGN DECISION:` marker as a hard stop, not a hint.

use serde::{Deserialize, Serialize};

use crate::money::Money;

/// The budgeting paradigm options under consideration.
///
/// DESIGN DECISION: exactly one of these (or an explicit hybrid) must be chosen
/// before the domain relationships and schema are finalized. This enum exists to
/// document the options, not to imply the decision is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BudgetingModel {
    /// Zero-based: every unit of money is allocated into a category envelope.
    Envelope,
    /// Target/limit-based: categories have goals; money is not moved.
    Flexible,
    /// A mix of envelope and flexible categories.
    Hybrid,
}

/// Stable identifier for a domain entity.
///
/// TODO: ID strategy (autoincrement vs. UUID) is a schema decision deferred
/// alongside the budgeting model; a plain `u64` is a placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Id(pub u64);

/// A budgeting period.
///
/// DESIGN DECISION: a period is modeled as a calendar month for now, but whether
/// periods are strict calendar months or user-defined pay cycles is open
/// (`docs/DESIGN.md` §1). Carry-over semantics between periods are likewise tied
/// to the budgeting-model decision and are intentionally absent here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Month {
    /// Four-digit year, e.g. `2026`.
    pub year: i32,
    /// Month of the year, `1..=12`.
    pub month: u8,
}

/// A spending category.
///
/// TODO: grouping (category groups), ordering, archival, and the link to
/// [`Budget`] are deferred until the budgeting model is chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    /// Stable identifier.
    pub id: Id,
    /// Human-readable name.
    pub name: String,
}

/// A budget entry for a category within a period.
///
/// DESIGN DECISION: the meaning of `allocated` is **undecided** — under the
/// envelope model it is money physically assigned to an envelope (with
/// carry-over); under the flexible model it is a target/limit only. The links
/// to [`Month`] and [`Category`] and any derived balance are deliberately left
/// out until that is settled. Do not attach behavior to this type yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    /// Stable identifier.
    pub id: Id,
    /// The amount budgeted for the period — semantics TBD (see above).
    pub allocated: Money,
}

/// A recorded money movement.
///
/// DESIGN DECISION: how a transaction relates to a [`Category`] / [`Month`] /
/// [`Budget`] (single category vs. splits, transfers between envelopes, income
/// as a first-class "to be budgeted" pool) depends on the budgeting model and is
/// intentionally not modeled yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    /// Stable identifier.
    pub id: Id,
    /// Signed amount: negative for spending, positive for income.
    pub amount: Money,
    /// Free-form memo.
    pub note: String,
}

#[cfg(test)]
mod tests {
    use super::{Budget, BudgetingModel, Category, Id, Money, Month, Transaction};

    // These tests only assert that the stub types are constructible and
    // (de)serialize. They deliberately assert *nothing* about budgeting
    // behavior, which is undecided.

    #[test]
    fn entities_are_constructible() {
        let _ = Month {
            year: 2026,
            month: 5,
        };
        let _ = Category {
            id: Id(1),
            name: "Groceries".to_owned(),
        };
        let _ = Budget {
            id: Id(1),
            allocated: Money::from_minor_units(50_000, 2),
        };
        let _ = Transaction {
            id: Id(1),
            amount: Money::from_minor_units(-1_299, 2),
            note: "Coffee".to_owned(),
        };
    }

    #[test]
    fn budgeting_model_round_trips_as_json() {
        for model in [
            BudgetingModel::Envelope,
            BudgetingModel::Flexible,
            BudgetingModel::Hybrid,
        ] {
            let json = serde_json::to_string(&model).unwrap();
            let back: BudgetingModel = serde_json::from_str(&json).unwrap();
            assert_eq!(model, back);
        }
    }
}
