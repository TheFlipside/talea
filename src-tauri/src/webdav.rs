//! Minimal `WebDAV` client for Nextcloud backup/restore.
//!
//! Only what the backup feature needs: PUT/GET a single file, create the parent
//! directory, and a connectivity/credentials check. HTTPS is required; the app
//! password is sent via HTTP Basic auth and never logged.

use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Method, StatusCode, Url};

/// Directory and filename of the backup on the server (under the user's files).
const BACKUP_DIR: &str = "Talea";
const BACKUP_FILE: &str = "talea-backup.sqlite3";

/// Hard ceiling on a backup, both ways. A download may be no bigger, so a
/// wrong or hostile server can't exhaust memory, and nor may an upload, so no
/// backup is made that restore would then refuse. A real Talea database is far
/// smaller (250,000 entries with short notes come to about 35 MB), and since a
/// restored ledger is read whole for every month view, this also bounds the
/// memory one can take.
const MAX_BACKUP_BYTES: u64 = 64 * 1024 * 1024;

/// Whether a backup of `len` bytes is past [`MAX_BACKUP_BYTES`].
fn too_large(len: u64) -> bool {
    len > MAX_BACKUP_BYTES
}

/// A `WebDAV` failure. Every message is safe to show the user and never contains
/// the password.
#[derive(Debug, thiserror::Error)]
pub enum WebDavError {
    /// The configured address is missing, malformed, or not `https://`.
    #[error("Enter a valid https:// Nextcloud address.")]
    InvalidUrl,
    /// A new server's address is valid, but not one the confirmation can
    /// show plainly (see [`server_address`]).
    #[error(
        "Use a plain https:// address of up to {max} characters: the server's name, then \
         a path of letters, digits and - . _ ~ / only, with no login, query or spaces.",
        max = MAX_ADDRESS_LEN
    )]
    NotPlainAddress,
    /// The server could not be reached (DNS, TLS, timeout, offline).
    #[error("Couldn't reach Nextcloud. Check the address and your connection.")]
    Network,
    /// The username or app password was rejected.
    #[error("Nextcloud rejected the username or app password.")]
    Auth,
    /// No backup file exists on the server yet.
    #[error("No backup was found on Nextcloud yet.")]
    NotFound,
    /// The backup on the server exceeds the size we're willing to download.
    #[error("The backup on Nextcloud is too large to download.")]
    TooLarge,
    /// This device's data is past the size a backup may be.
    #[error("Your data is over {max} MiB, too large to back up.", max = MAX_BACKUP_BYTES / 1024 / 1024)]
    TooLargeToUpload,
    /// Any other non-success HTTP status.
    #[error("Nextcloud returned an unexpected error ({0}).")]
    Server(u16),
}

/// Builds the `WebDAV` URL for `segments` under a user's files root:
/// `<base>/remote.php/dav/files/<user>/<segments…>` (segments are encoded).
fn dav_url(base: &Url, user: &str, segments: &[&str]) -> Result<Url, WebDavError> {
    let mut url = base.clone();
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|()| WebDavError::InvalidUrl)?;
        // Tolerate a base with or without a trailing slash.
        path.pop_if_empty();
        path.extend(["remote.php", "dav", "files", user]);
        path.extend(segments);
    }
    Ok(url)
}

/// Builds a rustls client config that trusts the bundled Mozilla root set
/// (`webpki-roots`) using the `ring` provider.
///
/// We supply this explicitly instead of letting reqwest pick its default
/// verifier: reqwest 0.13's rustls default is `rustls-platform-verifier`, which
/// on Android (and iOS) must be initialized with the platform's JNI/Context
/// before first use — without that it panics, aborting the app. Bundled roots
/// need no platform initialization and behave identically on every target.
fn tls_config() -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("ring provider supports the default TLS protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth()
}

/// Maps an HTTP status to our error set; 2xx and 207 Multi-Status are success.
fn check_status(status: StatusCode) -> Result<(), WebDavError> {
    if status.is_success() || status == StatusCode::MULTI_STATUS {
        return Ok(());
    }
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(WebDavError::Auth),
        StatusCode::NOT_FOUND => Err(WebDavError::NotFound),
        other => Err(WebDavError::Server(other.as_u16())),
    }
}

/// Whether `c` shows as itself: not a control character, nor a format
/// character such as a soft hyphen, which the parser drops from a host without
/// a word. Quotes and backslashes print fine; only `escape_debug` escapes them.
fn prints_as_itself(c: char) -> bool {
    matches!(c, '\'' | '"' | '\\') || c.escape_debug().eq([c])
}

/// The longest address a new server may have. The confirmation shows it whole,
/// on a phone-width dialog, and a Nextcloud address is far shorter.
const MAX_ADDRESS_LEN: usize = 128;

/// The longest label (the text between two dots) DNS can resolve.
const MAX_DNS_LABEL_LEN: usize = 63;

/// Parses a Nextcloud server address, requiring `https://`.
fn https_base(base_url: &str) -> Result<Url, WebDavError> {
    let base = Url::parse(base_url.trim()).map_err(|_| WebDavError::InvalidUrl)?;
    if base.scheme() != "https" {
        return Err(WebDavError::InvalidUrl);
    }
    Ok(base)
}

/// `base_url` as the user confirms it before backups go there: the address in
/// the form requests use. The parser has already turned an international host
/// name into its ASCII form, so a look-alike letter from another script can't
/// pass for a familiar host, and dropped the default port.
///
/// It must name the server and nothing more, short enough to show whole: a
/// login (`user@host`) would put a familiar name before the real host, a long
/// label could push the real host off the dialog, and the path counts as much
/// as the host, since one host can serve several Nextclouds. The path may hold
/// only letters, digits, `-`, `.`, `_`, `~` and `/`, all a Nextcloud's needs:
/// an `@` or `:` in it (`https://evil.example/@good.example`) would dress the
/// path up as a familiar host. Every byte must
/// print as itself, and none may be a `%`: the Linux dialog reads its text as
/// a `printf` format, where `%s` reads memory it was never given. The address
/// as typed, which is what is stored, may hold no whitespace or control
/// character either, since the parser drops some without a word, and so no
/// more than the shown one can it run long.
///
/// # Errors
/// [`WebDavError::InvalidUrl`] if `base_url` isn't an `https://` address at
/// all, [`WebDavError::NotPlainAddress`] if it isn't such a plain one.
pub fn server_address(base_url: &str) -> Result<String, WebDavError> {
    let typed = base_url.trim();
    if typed.chars().count() > MAX_ADDRESS_LEN {
        return Err(WebDavError::NotPlainAddress);
    }
    let base = https_base(typed)?;
    if typed
        .chars()
        .any(|c| c.is_whitespace() || !prints_as_itself(c))
    {
        return Err(WebDavError::NotPlainAddress);
    }
    let host = base.host_str().unwrap_or_default();
    let names_only_a_server = base.username().is_empty()
        && base.password().is_none()
        && base.query().is_none()
        && base.fragment().is_none();
    let address = match base.path() {
        "/" => base.as_str().trim_end_matches('/'),
        _ => base.as_str(),
    };
    let plain_path = base
        .path()
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte));
    // Letters, digits, `-` and `.`, or an IPv6 literal in brackets: the
    // parser lets other punctuation through, but no real host has it.
    let plain_host = host.starts_with('[')
        || host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.');
    let acceptable = names_only_a_server
        && plain_path
        && plain_host
        && !host.is_empty()
        && host
            .split('.')
            .all(|label| label.len() <= MAX_DNS_LABEL_LEN)
        && address.len() <= MAX_ADDRESS_LEN
        && address
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'%');
    if acceptable {
        Ok(address.to_owned())
    } else {
        Err(WebDavError::NotPlainAddress)
    }
}

/// A configured Nextcloud `WebDAV` endpoint.
pub struct WebDav {
    client: Client,
    base: Url,
    user: String,
    password: String,
}

impl WebDav {
    /// Builds a client for `base_url` (the Nextcloud server root). Requires
    /// `https://`.
    pub fn new(base_url: &str, user: &str, password: &str) -> Result<Self, WebDavError> {
        let base = https_base(base_url)?;
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            // Don't follow redirects: a redirect could forward the Basic-auth
            // credentials to another host. WebDAV has no need to redirect.
            .redirect(reqwest::redirect::Policy::none())
            // Use our bundled-roots config (see `tls_config`) instead of the
            // default platform verifier, which isn't initialized on mobile.
            .use_preconfigured_tls(tls_config())
            .build()
            .map_err(|_| WebDavError::Network)?;
        Ok(Self {
            client,
            base,
            user: user.to_owned(),
            password: password.to_owned(),
        })
    }

    fn request(&self, method: Method, url: Url) -> reqwest::RequestBuilder {
        self.client
            .request(method, url)
            .basic_auth(&self.user, Some(&self.password))
    }

    async fn send(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, WebDavError> {
        builder.send().await.map_err(|_| WebDavError::Network)
    }

    fn method(name: &[u8]) -> Method {
        Method::from_bytes(name).expect("static WebDAV method is valid")
    }

    /// Verifies the server and credentials (PROPFIND the user's files root).
    pub async fn check(&self) -> Result<(), WebDavError> {
        let url = dav_url(&self.base, &self.user, &[])?;
        let req = self
            .request(Self::method(b"PROPFIND"), url)
            .header("Depth", "0");
        check_status(self.send(req).await?.status())
    }

    /// Creates the backup directory; an existing directory is fine.
    async fn ensure_dir(&self) -> Result<(), WebDavError> {
        let url = dav_url(&self.base, &self.user, &[BACKUP_DIR])?;
        let status = self
            .send(self.request(Self::method(b"MKCOL"), url))
            .await?
            .status();
        // 405 Method Not Allowed == the collection already exists.
        if status == StatusCode::METHOD_NOT_ALLOWED {
            return Ok(());
        }
        check_status(status)
    }

    /// Uploads the backup snapshot, creating the directory if needed, unless it
    /// is past [`MAX_BACKUP_BYTES`].
    pub async fn put_backup(&self, bytes: Vec<u8>) -> Result<(), WebDavError> {
        if too_large(bytes.len() as u64) {
            return Err(WebDavError::TooLargeToUpload);
        }
        self.ensure_dir().await?;
        let url = dav_url(&self.base, &self.user, &[BACKUP_DIR, BACKUP_FILE])?;
        let req = self.request(Method::PUT, url).body(bytes);
        check_status(self.send(req).await?.status())
    }

    /// Downloads the backup snapshot, bounded by [`MAX_BACKUP_BYTES`] so a
    /// wrong/hostile server can't exhaust memory. The body is read in chunks and
    /// the running total is capped (covering chunked responses with no
    /// advertised length).
    pub async fn get_backup(&self) -> Result<Vec<u8>, WebDavError> {
        let url = dav_url(&self.base, &self.user, &[BACKUP_DIR, BACKUP_FILE])?;
        let mut resp = self.send(self.request(Method::GET, url)).await?;
        check_status(resp.status())?;
        if resp.content_length().is_some_and(too_large) {
            return Err(WebDavError::TooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|_| WebDavError::Network)? {
            if too_large(body.len() as u64 + chunk.len() as u64) {
                return Err(WebDavError::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(base: &str, user: &str, segments: &[&str]) -> String {
        dav_url(&Url::parse(base).unwrap(), user, segments)
            .unwrap()
            .to_string()
    }

    #[test]
    fn builds_the_user_files_path() {
        assert_eq!(
            url(
                "https://cloud.example.com",
                "max",
                &[BACKUP_DIR, BACKUP_FILE]
            ),
            "https://cloud.example.com/remote.php/dav/files/max/Talea/talea-backup.sqlite3"
        );
    }

    #[test]
    fn tolerates_a_trailing_slash_on_the_base() {
        assert_eq!(
            url("https://cloud.example.com/", "max", &[BACKUP_DIR]),
            "https://cloud.example.com/remote.php/dav/files/max/Talea"
        );
    }

    #[test]
    fn encodes_the_username() {
        assert_eq!(
            url("https://cloud.example.com", "a b", &[]),
            "https://cloud.example.com/remote.php/dav/files/a%20b"
        );
    }

    #[test]
    fn rejects_non_https() {
        assert!(matches!(
            WebDav::new("http://cloud.example.com", "u", "p"),
            Err(WebDavError::InvalidUrl)
        ));
    }

    #[test]
    fn a_server_address_shows_where_a_backup_would_go() {
        for (base_url, address) in [
            ("https://cloud.example.com", "https://cloud.example.com"),
            (
                " https://Cloud.Example.com:443/ ",
                "https://cloud.example.com",
            ),
            (
                "https://cloud.example.com:8443",
                "https://cloud.example.com:8443",
            ),
            // One host can serve several Nextclouds, so the path shows too.
            (
                "https://shared.example/~mallory/",
                "https://shared.example/~mallory/",
            ),
            // Hosts appear in the one form requests use.
            ("https://bücher.example", "https://xn--bcher-kva.example"),
            ("https://ex%61mple.com", "https://example.com"),
            ("https://0x7f.1", "https://127.0.0.1"),
            ("https://[::1]:8443", "https://[::1]:8443"),
            ("https://good.example./", "https://good.example."),
        ] {
            assert_eq!(server_address(base_url).unwrap(), address, "{base_url}");
        }
        let label = "a".repeat(MAX_DNS_LABEL_LEN);
        assert!(server_address(&format!("https://{label}.example")).is_ok());
        let longest = format!("https://example.com/{}", "a".repeat(MAX_ADDRESS_LEN - 20));
        assert!(server_address(&longest).is_ok());
        // Not an `https://` address at all.
        for base_url in ["http://cloud.example.com", "cloud.example.com", ""] {
            assert!(
                matches!(server_address(base_url), Err(WebDavError::InvalidUrl)),
                "{base_url}"
            );
        }
        // An `https://` address, but not one the confirmation can show plainly.
        for base_url in [
            // A login before the host, or an `@` after it, could pass for
            // part of the host.
            "https://cloud.example.com@evil.example/",
            "https://evil.example/@good.example",
            r"https://evil.example\@good.example",
            "https://evil.example/good.example:443",
            "https://max:secret@cloud.example.com",
            "https://cloud.example.com/?next=evil.example",
            "https://cloud.example.com/#evil.example",
            // Characters that would break or reorder the dialog's lines.
            "https://good.example%0a.evil.example",
            "https://good.example%e2%80%ae.evil.example",
            "https://good.example\u{2028}.evil.example",
            "https://good.example／evil.example",
            "https://[fe80::1%25eth0]",
            // A path that needs percent-encoding, which the dialog can't show.
            "https://cloud.example.com/a b",
            "https://cloud.example.com/%s%n",
            // Typed longer than shown (the escapes decode to plain letters),
            // and the typed text is what is stored.
            &format!("https://{}.example", "%61".repeat(40)),
            // Punctuation no real host has.
            "https://clo!ud.example.com",
            "https://cloud.example.com&evil.example",
            // Characters the parser would drop, leaving more stored than shown.
            "https://cl\u{ad}oud.example.com",
            "https://cl\u{200d}oud.example.com",
            "https://cloud.example.com/\tnextcloud",
            "https://cloud.\nexample.com",
            "https://cloud.example.com/next cloud",
            // A label DNS can't resolve, padding the real host out of sight.
            &format!("https://cloud.example.com.{label}a.evil.example"),
            &format!("{longest}a"),
        ] {
            assert!(
                matches!(
                    server_address(base_url),
                    Err(WebDavError::InvalidUrl | WebDavError::NotPlainAddress)
                ),
                "{base_url}"
            );
        }
        for base_url in [
            "https://cloud.example.com/next cloud",
            "https://evil.example/@good.example",
            &format!("{longest}a"),
        ] {
            assert!(
                matches!(server_address(base_url), Err(WebDavError::NotPlainAddress)),
                "{base_url}"
            );
        }
    }

    #[test]
    fn a_backup_may_be_64_mib_and_no_more() {
        assert!(!too_large(64 * 1024 * 1024));
        assert!(too_large(64 * 1024 * 1024 + 1));
        assert_eq!(
            WebDavError::TooLargeToUpload.to_string(),
            "Your data is over 64 MiB, too large to back up."
        );
    }
}
