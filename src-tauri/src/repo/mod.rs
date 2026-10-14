//! Repository layer: maps between `SQLite` rows and `talea_core` domain types.
//!
//! Functions are grouped per entity (`account`, `budget`, `category`, `entry`,
//! `rule`, `skip`), plus `ledger`, which reads one account's inputs together.
//! Reads reconstruct domain values through their public validating
//! constructors; a failure there means the stored data is corrupt
//! ([`RepoError::Corrupt`](crate::error::RepoError::Corrupt)), since the write
//! paths only ever persist already-valid values.

pub mod account;
pub mod budget;
pub mod category;
pub mod entry;
pub mod ledger;
pub mod rule;
pub mod skip;

mod map;
