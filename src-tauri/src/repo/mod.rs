//! Repository layer: maps between `SQLite` rows and `talea_core` domain types.
//!
//! Functions are grouped per entity (`account`, `category`, `entry`, `rule`).
//! Reads reconstruct domain values through their public validating
//! constructors; a failure there means the stored data is corrupt
//! ([`RepoError::Corrupt`](crate::error::RepoError::Corrupt)), since the write
//! paths only ever persist already-valid values.

pub mod account;
pub mod category;
pub mod entry;
pub mod rule;
pub mod skip;

mod map;
