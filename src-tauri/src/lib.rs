//! The Talea Tauri shell.
//!
//! This crate is the thin boundary between the pure [`talea_core`] domain and
//! the React frontend. It owns platform integration and (later) persistence;
//! the frontend reaches it only through Tauri commands. Keep domain logic out of
//! here — it belongs in `talea-core`.

use serde::Serialize;
use tauri::Manager;

use talea_core::Money;

mod backup;
mod commands;
mod db;
mod dto;
mod error;
mod repo;
mod webdav;

#[cfg(test)]
mod tests;

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
/// Panics if the Tauri runtime fails to build or start, or if the database
/// cannot be opened/migrated in the `setup` hook. These are fatal, unrecoverable
/// startup errors — the app refuses to run rather than operate on a broken
/// database.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Install the ring crypto provider as rustls's process default. The WebDAV
    // (Nextcloud) client builds its config with ring explicitly (see
    // `webdav.rs`), so this is a safety net for any other rustls path that falls
    // back to the process default. ring keeps the iOS/Android cross-compile free
    // of aws-lc-rs's C/cmake toolchain. Best-effort: a prior install is harmless.
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Status-bar theming and the widget publisher work on all platforms (no-ops
    // on desktop), so they're registered unconditionally.
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_statusbar::init())
        .plugin(tauri_plugin_budgetwidget::init());
    // The biometric app lock is mobile-only; the plugin isn't built into the
    // desktop binary (see Cargo.toml), so the frontend just sees it unavailable.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_biometric::init());
    builder
        .setup(|app| {
            // Open (creating if needed) and migrate the on-device database, then
            // share the pool with all commands. Async work is driven on Tauri's
            // runtime; failure aborts startup.
            let app_data_dir = app.path().app_data_dir()?;
            let pool = tauri::async_runtime::block_on(db::init_pool(&app_data_dir))?;
            app.manage(pool);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            smoke_check,
            commands::create_account,
            commands::list_accounts,
            commands::update_account,
            commands::delete_account,
            commands::create_category,
            commands::list_categories,
            commands::update_category,
            commands::delete_category,
            commands::create_entry,
            commands::list_entries,
            commands::update_entry,
            commands::delete_entry,
            commands::create_transfer,
            commands::create_rule,
            commands::list_rules,
            commands::update_rule,
            commands::delete_rule,
            commands::month_summary,
            commands::summaries_for_range,
            commands::expenses_by_category,
            commands::month_occurrences,
            commands::skip_occurrence,
            commands::detach_occurrence,
            commands::nextcloud_get_config,
            commands::nextcloud_set_config,
            commands::nextcloud_test,
            commands::backup_now,
            commands::restore_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
