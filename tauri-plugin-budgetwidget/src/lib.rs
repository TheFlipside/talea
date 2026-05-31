//! Tauri plugin: publish an **abstract** budget-health snapshot to the OS shared
//! storage a home-screen widget reads.
//!
//! Only a ring fraction (0..1), a derived percent, an overspent flag and the
//! account name ever cross this boundary — never any monetary figure. The real
//! numbers stay in the app, behind the optional biometric lock. The frontend
//! computes the fractions (reusing its budget-ring view model) and calls
//! `publish_health`; the native side writes them to Android `SharedPreferences`
//! / an iOS App Group and nudges the widgets to redraw. On desktop there is no
//! widget surface, so it is a no-op.

use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, Runtime,
};

#[cfg(mobile)]
use tauri::plugin::PluginHandle;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_budgetwidget);

/// One account's abstract budget health for the current month. Carries no money.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AccountHealth {
    /// Account id, as a string (typed `AccountId` crosses the IPC as a number,
    /// but the widget only needs an opaque key to match the chosen account).
    pub id: String,
    /// Account label, shown in the widget's config picker and name caption.
    pub name: String,
    /// Ring fill fraction, 0..=1 (already clamped by the frontend).
    pub fraction: f64,
    /// Whether the month is overspent (tints the ring).
    pub overspent: bool,
}

/// The full snapshot published to shared storage: one entry per account.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HealthPayload {
    pub accounts: Vec<AccountHealth>,
}

/// Plugin state: the handle to the native (mobile) implementation.
struct BudgetWidget<R: Runtime> {
    #[allow(dead_code)] // kept for a stable Send+Sync state type; unused on desktop
    app: AppHandle<R>,
    #[cfg(mobile)]
    handle: PluginHandle<R>,
}

impl<R: Runtime> BudgetWidget<R> {
    // On desktop this is intentionally a trivial no-op; the lints below fire only
    // for that build, where there is no `self`/payload/error to use.
    #[cfg_attr(
        desktop,
        allow(
            clippy::unused_self,
            clippy::unnecessary_wraps,
            clippy::needless_pass_by_value
        )
    )]
    fn publish(&self, payload: HealthPayload) -> Result<(), String> {
        #[cfg(mobile)]
        return self
            .handle
            .run_mobile_plugin::<()>("publishHealth", payload)
            .map_err(|e| e.to_string());
        #[cfg(desktop)]
        {
            let _ = payload; // no widget surface on desktop
            Ok(())
        }
    }
}

/// Defensive upper bound on accounts in one publish — far above any realistic
/// account count, so a malformed/runaway IPC call can't write an unbounded blob.
const MAX_ACCOUNTS: usize = 256;

#[tauri::command]
async fn publish_health<R: Runtime>(
    app: AppHandle<R>,
    payload: HealthPayload,
) -> Result<(), String> {
    if payload.accounts.len() > MAX_ACCOUNTS {
        return Err(format!(
            "too many accounts ({}, max {MAX_ACCOUNTS})",
            payload.accounts.len()
        ));
    }
    app.state::<BudgetWidget<R>>().publish(payload)
}

/// Initializes the plugin.
#[must_use]
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("budgetwidget")
        .invoke_handler(tauri::generate_handler![publish_health])
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let handle = _api.register_android_plugin(
                "com.luminaapps.talea.budgetwidget",
                "BudgetWidgetPlugin",
            )?;
            #[cfg(target_os = "ios")]
            let handle = _api.register_ios_plugin(init_plugin_budgetwidget)?;
            app.manage(BudgetWidget {
                app: app.clone(),
                #[cfg(mobile)]
                handle,
            });
            Ok(())
        })
        .build()
}
