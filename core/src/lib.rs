//! # Talea core
//!
//! Pure-Rust domain logic and money math for the Talea budget app. This crate is
//! deliberately free of Tauri, IO, and SQL so it stays portable and fully
//! unit-testable in isolation. The `src-tauri` shell depends on it; it depends
//! on nothing platform-specific.
//!
//! - [`money`] — exact, never-floating-point monetary values.
//! - [`domain`] — the budgeting domain model. **Currently stubbed**: the
//!   budgeting paradigm (envelope vs. flexible vs. hybrid) and the `SQLite` schema
//!   are open design decisions tracked in `docs/DESIGN.md`.

pub mod domain;
pub mod money;

pub use domain::{Budget, BudgetingModel, Category, Id, Month, Transaction};
pub use money::{Money, MoneyError};

/// The crate version, surfaced so the shell can display a build identifier in
/// the smoke screen without re-deriving it.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn version_is_reported() {
        assert!(!version().is_empty());
    }
}
