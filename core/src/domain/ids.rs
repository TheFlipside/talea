//! Typed entity identifiers.
//!
//! Each entity gets its own ID newtype so a `CategoryId` can never be passed
//! where an `AccountId` is expected. IDs (de)serialize as the bare `u64`
//! (`#[serde(transparent)]`) since they map to integer primary keys in the
//! persistence layer; the inner value is private and assigned by that layer.

use serde::{Deserialize, Serialize};

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            /// Wraps a raw identifier (assigned by the persistence layer).
            #[must_use]
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            /// The raw identifier value.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

define_id!(
    /// Identifies an [`Account`](crate::domain::Account).
    AccountId
);
define_id!(
    /// Identifies a [`Category`](crate::domain::Category).
    CategoryId
);
define_id!(
    /// Identifies an [`Entry`](crate::domain::Entry).
    EntryId
);
define_id!(
    /// Identifies a [`RecurringRule`](crate::domain::RecurringRule).
    RecurringRuleId
);

#[cfg(test)]
mod tests {
    use super::{AccountId, CategoryId};

    #[test]
    fn round_trips_as_bare_integer() {
        let id = AccountId::new(42);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "42");
        assert_eq!(serde_json::from_str::<AccountId>("42").unwrap(), id);
        assert_eq!(id.get(), 42);
    }

    #[test]
    fn distinct_types_do_not_mix() {
        // Compile-time guarantee documented here: the next line would not
        // compile — `AccountId` and `CategoryId` are distinct types.
        // let _: AccountId = CategoryId::new(1);
        assert_ne!(AccountId::new(1).get(), CategoryId::new(2).get());
    }
}
