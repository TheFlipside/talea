//! Keeps the webview on the app's own pages.
//!
//! A script running in the webview that shouldn't be, such as a compromised
//! dependency, can read the whole ledger through the app's commands. The CSP
//! stops it sending that anywhere by request (`connect-src`, `img-src`, …), but
//! not by leaving: navigating the page to a server of its own, the data in the
//! URL, or to a scheme another app handles (`mailto:`, `intent:`). So every
//! navigation the webview starts must stay on the app's own origin.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime, Url};

/// Whether Tauri serves the app over `http(s)://tauri.localhost` here, as on
/// Android and Windows, rather than over `tauri://localhost`. This and the two
/// origins below mirror where Tauri 2 serves a built frontend
/// (`tauri::manager::AppManager::tauri_protocol_url`, private): after a Tauri
/// upgrade, check them, or the guard refuses the app its own pages.
const SERVED_OVER_HTTP: bool = cfg!(any(windows, target_os = "android"));

/// The plugin refusing every navigation off the app's own pages.
pub(crate) fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("navigation")
        .on_navigation(|webview, url| {
            let dev_server = dev_server(tauri::is_dev(), webview.config().build.dev_url.as_ref());
            is_app_page(url, SERVED_OVER_HTTP, dev_server)
        })
        .build()
}

/// The dev server the app's pages come from, if any: the configured one, but
/// only in a dev build (`is_dev`), which is exactly when Tauri loads the
/// frontend from it. A built app, with its custom protocol, never does.
fn dev_server(is_dev: bool, configured: Option<&Url>) -> Option<&Url> {
    configured.filter(|_| is_dev)
}

/// Whether `url` is one of the app's own pages: where Tauri serves the built
/// frontend on this platform (`http(s)://tauri.localhost` if
/// `served_over_http`, else `tauri://localhost`), or, in a dev build, a page
/// of the `dev_server` the frontend runs on. The other platforms' origin is
/// none of the app's here, but an ordinary host name a resolver might send
/// anywhere.
fn is_app_page(url: &Url, served_over_http: bool, dev_server: Option<&Url>) -> bool {
    let built = match (url.scheme(), url.host_str(), url.port()) {
        ("tauri", Some("localhost"), None) => !served_over_http,
        ("http" | "https", Some("tauri.localhost"), None) => served_over_http,
        _ => false,
    };
    let dev = dev_server.is_some_and(|dev| {
        matches!(url.scheme(), "http" | "https") && dev.origin() == url.origin()
    });
    built || dev
}

#[cfg(test)]
mod tests {
    use tauri::Url;

    use super::{dev_server, is_app_page};

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    /// Whether `page` is the app's on a platform serving it over custom
    /// scheme, and on one serving it over HTTP.
    fn allowed(page: &str) -> (bool, bool) {
        (
            is_app_page(&url(page), false, None),
            is_app_page(&url(page), true, None),
        )
    }

    #[test]
    fn the_app_may_move_between_its_own_pages() {
        for page in [
            "tauri://localhost",
            "tauri://localhost/settings?tab=backup#top",
        ] {
            assert_eq!(allowed(page), (true, false), "{page}");
        }
        for page in [
            "http://tauri.localhost/",
            "https://tauri.localhost/index.html",
            // The default port is no port.
            "https://tauri.localhost:443/",
            "http://tauri.localhost:80/",
        ] {
            assert_eq!(allowed(page), (false, true), "{page}");
        }
    }

    #[test]
    fn it_may_not_leave_them() {
        for page in [
            "https://evil.example/?ledger=everything",
            "http://localhost:1420/",
            "tauri://evil.example",
            "tauri://LOCALHOST",
            "tauri://localhost:80",
            "http://tauri.localhost.evil.example/",
            "http://tauri.localhost./",
            "http://tauri.localhost:8080/",
            "http://tauri.localhost@evil.example/",
            "mailto:someone@evil.example?body=ledger",
            "intent://evil#Intent;scheme=https;end",
            "sms:+15550100?body=ledger",
            "file:///etc/passwd",
            "data:text/html,<p>hi</p>",
            "blob:tauri://localhost/1",
            "javascript:alert(1)",
            "about:blank",
        ] {
            assert_eq!(allowed(page), (false, false), "{page}");
        }
    }

    /// A config file of the shell's, parsed.
    fn config(file: &str) -> serde_json::Value {
        let path = format!("{}/{file}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn only_the_platforms_fetching_ipc_over_http_may_connect_to_it() {
        // Windows runs its IPC over `http://ipc.localhost`, and Android fetches
        // channel data there; elsewhere it is an ordinary host name a resolver
        // might send anywhere. So the base policy leaves it out, and those two
        // platforms' files add it to `connect-src` and change nothing else.
        let base = config("tauri.conf.json");
        let csp = &base["app"]["security"]["csp"];
        assert!(!csp.to_string().contains("ipc.localhost"), "{csp}");
        let connect = csp["connect-src"].as_str().unwrap();
        for platform in ["tauri.windows.conf.json", "tauri.android.conf.json"] {
            assert_eq!(
                config(platform),
                serde_json::json!({
                    "$schema": "https://schema.tauri.app/config/2",
                    "app": { "security": { "csp": {
                        "connect-src": format!("{connect} http://ipc.localhost")
                    } } }
                }),
                "{platform}"
            );
        }
    }

    #[test]
    fn only_a_dev_build_has_a_dev_server() {
        let configured = url("http://localhost:1420");
        assert_eq!(dev_server(true, Some(&configured)), Some(&configured));
        assert_eq!(dev_server(false, Some(&configured)), None);
        assert_eq!(dev_server(true, None), None);
    }

    #[test]
    fn a_dev_build_may_also_use_its_dev_server() {
        let dev = url("http://localhost:1420");
        for served_over_http in [false, true] {
            let is_app = |page: &str| is_app_page(&url(page), served_over_http, Some(&dev));
            assert!(is_app("http://localhost:1420/settings"));
            for page in [
                "http://localhost:1421/",
                "https://localhost:1420/",
                "http://127.0.0.1:1420/",
                "https://evil.example/",
                // Its origin is the dev server's, but it is no page of it.
                "blob:http://localhost:1420/1",
            ] {
                assert!(!is_app(page), "{page}");
            }
        }
    }
}
