//! Minimal `WebDAV` client for Nextcloud backup/restore.
//!
//! Only what the backup feature needs: PUT/GET a single file, create the parent
//! directory, and a connectivity/credentials check. HTTPS is required; the app
//! password is sent via HTTP Basic auth and never logged.

use std::time::Duration;

use reqwest::{Client, Method, StatusCode, Url};

/// Directory and filename of the backup on the server (under the user's files).
const BACKUP_DIR: &str = "Talea";
const BACKUP_FILE: &str = "talea-backup.sqlite3";

/// Hard ceiling on a downloaded backup, so a wrong/hostile server can't OOM the
/// device. A real Talea database is far smaller; 256 MiB is generous headroom.
const MAX_BACKUP_BYTES: u64 = 256 * 1024 * 1024;

/// A `WebDAV` failure. Every message is safe to show the user and never contains
/// the password.
#[derive(Debug, thiserror::Error)]
pub enum WebDavError {
    /// The configured address is missing, malformed, or not `https://`.
    #[error("Enter a valid https:// Nextcloud address.")]
    InvalidUrl,
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
        let base = Url::parse(base_url.trim()).map_err(|_| WebDavError::InvalidUrl)?;
        if base.scheme() != "https" {
            return Err(WebDavError::InvalidUrl);
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            // Don't follow redirects: a redirect could forward the Basic-auth
            // credentials to another host. WebDAV has no need to redirect.
            .redirect(reqwest::redirect::Policy::none())
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

    /// Uploads the backup snapshot, creating the directory if needed.
    pub async fn put_backup(&self, bytes: Vec<u8>) -> Result<(), WebDavError> {
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
        if resp
            .content_length()
            .is_some_and(|len| len > MAX_BACKUP_BYTES)
        {
            return Err(WebDavError::TooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|_| WebDavError::Network)? {
            if body.len() as u64 + chunk.len() as u64 > MAX_BACKUP_BYTES {
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
}
