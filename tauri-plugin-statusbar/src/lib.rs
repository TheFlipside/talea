//! Tauri plugin: set the system status/navigation bar icon appearance to match
//! the app's theme.
//!
//! The OS status bar belongs to the native activity / view controller, not the
//! web view, so it can't be styled from the frontend. This plugin exposes a
//! single `set_dark` command: pass `true` when the app is showing its dark theme
//! (the bar then uses **light** icons) and `false` for the light theme (**dark**
//! icons). On desktop it is a no-op (there is no system bar to style).

#[cfg(mobile)]
use serde::Serialize;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, Runtime,
};

#[cfg(mobile)]
use tauri::plugin::PluginHandle;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_statusbar);

#[cfg(mobile)]
#[derive(Serialize)]
struct SetDarkPayload {
    dark: bool,
}

/// Plugin state: the handle to the native (mobile) implementation.
struct Statusbar<R: Runtime> {
    #[allow(dead_code)] // kept for a stable Send+Sync state type; unused on desktop
    app: AppHandle<R>,
    #[cfg(mobile)]
    handle: PluginHandle<R>,
}

impl<R: Runtime> Statusbar<R> {
    // On desktop this is intentionally a trivial no-op; the lints below fire only
    // for that build, where there is no `self`/error to use.
    #[cfg_attr(desktop, allow(clippy::unused_self, clippy::unnecessary_wraps))]
    fn set_dark(&self, dark: bool) -> Result<(), String> {
        #[cfg(mobile)]
        return self
            .handle
            .run_mobile_plugin::<()>("setDark", SetDarkPayload { dark })
            .map_err(|e| e.to_string());
        #[cfg(desktop)]
        {
            let _ = dark; // no system bar on desktop
            Ok(())
        }
    }
}

#[tauri::command]
async fn set_dark<R: Runtime>(app: AppHandle<R>, dark: bool) -> Result<(), String> {
    app.state::<Statusbar<R>>().set_dark(dark)
}

/// Initializes the plugin.
#[must_use]
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("statusbar")
        .invoke_handler(tauri::generate_handler![set_dark])
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let handle =
                _api.register_android_plugin("com.luminaapps.talea.statusbar", "StatusbarPlugin")?;
            #[cfg(target_os = "ios")]
            let handle = _api.register_ios_plugin(init_plugin_statusbar)?;
            app.manage(Statusbar {
                app: app.clone(),
                #[cfg(mobile)]
                handle,
            });
            Ok(())
        })
        .build()
}
