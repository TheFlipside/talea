//! Locking a mutex whose poisoning can be ignored.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Takes `mutex`, ignoring poisoning: for a mutex that guards nothing, or
/// nothing a panic could leave half-written for the next taker.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
