//! The Talea Tauri shell.
//!
//! This crate is the thin boundary between the pure [`talea_core`] domain and
//! the React frontend. It owns platform integration and (later) persistence;
//! the frontend reaches it only through Tauri commands. Keep domain logic out of
//! here — it belongs in `talea-core`.

use serde::Serialize;
use talea_core::Money;

/// Payload for the smoke-screen command, proving the `core → shell → frontend`
/// bridge end to end. Note `sample_amount` is a [`Money`], which serializes as a
/// **string**, never a JSON number.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SmokeInfo {
    greeting: String,
    core_version: String,
    sample_amount: Money,
}

/// Returns a greeting plus a sample [`Money`] value sourced from `talea-core`.
///
/// This exists purely to verify the wiring; it carries no budgeting logic.
#[tauri::command]
fn smoke_check(name: &str) -> SmokeInfo {
    // Bound untrusted IPC input before allocating with it. Cap by chars, not
    // bytes, so we never slice inside a multi-byte UTF-8 sequence. This sets the
    // expected pattern for every command that follows.
    const MAX_NAME_LEN: usize = 256;
    let name: String = name.trim().chars().take(MAX_NAME_LEN).collect();
    let who = if name.is_empty() {
        "world"
    } else {
        name.as_str()
    };

    SmokeInfo {
        greeting: format!("Hello, {who} — Talea is wired up."),
        core_version: talea_core::version().to_owned(),
        // Exercised through the typed boundary; crosses to JS as "1234.56".
        sample_amount: Money::from_minor_units(123_456, 2),
    }
}

/// Builds and runs the Tauri application. Shared by the desktop binary and the
/// mobile (Android/iOS) entry points.
///
/// # Panics
///
/// Panics if the Tauri runtime fails to build or start (for example, a missing
/// or invalid generated context). This is a fatal, unrecoverable startup error.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![smoke_check])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
