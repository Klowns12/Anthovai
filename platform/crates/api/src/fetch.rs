//! Fetching a URL the customer gave us.
//!
//! This is the one place in the platform where a customer decides what address
//! our servers connect to, which makes it the one place server-side request
//! forgery is possible. A URL that resolves to `169.254.169.254` reaches the
//! cloud metadata service and its credentials; one that resolves to `127.0.0.1`
//! reaches our own admin ports. Neither is reachable from the customer's
//! network — that is exactly why they would ask us to fetch it.
//!
//! So the address is checked, not the name, and redirects are followed one at a
//! time with the guard applied to each hop rather than left to the HTTP client.
//! The guard itself lives in `anthovai_knowledge::url_guard`, because the upload
//! endpoint has to make the same decision before it accepts the URL at all.

use std::time::Duration;

use anthovai_core::{DomainError, Result};
use anthovai_knowledge::url_guard::allowed;
use url::Url;

use anthovai_ingestion::error_codes;

/// The whole fetch, including every redirect.
const TIMEOUT: Duration = Duration::from_secs(15);

/// The most we will download. Checked against the declared length first and
/// against the bytes as they arrive, because the header is the server's claim.
const MAX_BYTES: usize = 10 * 1024 * 1024;

/// How many redirects to follow. Enough for the http→https→www chain every real
/// site has, and short enough that a redirect loop ends quickly.
const MAX_REDIRECTS: usize = 5;

/// What came back.
pub struct Fetched {
    /// The address actually fetched, after redirects. This is what belongs in
    /// the document's metadata — not what the customer typed.
    pub final_url: String,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// Fetch a customer-supplied URL, refusing anything that points inward.
pub async fn fetch(client: &reqwest::Client, url: &str) -> Result<Fetched> {
    let mut current = allowed(url)?;

    for _ in 0..=MAX_REDIRECTS {
        let response = client
            .get(current.clone())
            .timeout(TIMEOUT)
            .send()
            .await
            .map_err(|e| unreachable(&current, e))?;

        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| {
                    DomainError::validation(format!(
                        "{}: {current} redirected without saying where to",
                        error_codes::FETCH_FAILED
                    ))
                })?;

            // Resolved against the current URL, so a relative `Location` works,
            // and then checked again from scratch. A public page redirecting to
            // `http://169.254.169.254/` is the attack this stops.
            let next = current.join(location).map_err(|_| {
                DomainError::validation(format!(
                    "{}: {current} redirected to something that is not a URL",
                    error_codes::FETCH_FAILED
                ))
            })?;
            current = allowed(next.as_str())?;
            continue;
        }

        if !status.is_success() {
            return Err(DomainError::validation(format!(
                "{}: {current} answered {}",
                error_codes::FETCH_FAILED,
                status.as_u16()
            )));
        }

        if let Some(length) = response.content_length() {
            if length as usize > MAX_BYTES {
                return Err(too_large(length as usize));
            }
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(';').next().unwrap_or(v).trim().to_lowercase());

        // Streamed rather than read whole: a server that lies about its
        // `Content-Length` should not be able to fill our memory.
        let mut bytes = Vec::new();
        let mut stream = response;
        while let Some(chunk) = stream.chunk().await.map_err(|e| unreachable(&current, e))? {
            if bytes.len() + chunk.len() > MAX_BYTES {
                return Err(too_large(bytes.len() + chunk.len()));
            }
            bytes.extend_from_slice(&chunk);
        }

        return Ok(Fetched {
            final_url: current.to_string(),
            content_type,
            bytes,
        });
    }

    Err(DomainError::validation(format!(
        "{}: too many redirects",
        error_codes::FETCH_FAILED
    )))
}

/// A client configured for this and nothing else.
///
/// Redirects are turned off at the client so each hop comes back to us and is
/// checked; letting `reqwest` follow them would skip the guard entirely.
pub fn client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .connect_timeout(Duration::from_secs(5))
        .user_agent("Anthovai-Ingest/1.0 (+https://www.anthovai.com/bot)")
        .build()
}

fn unreachable(url: &Url, error: reqwest::Error) -> DomainError {
    let reason = if error.is_timeout() {
        "it did not answer in time".to_owned()
    } else if error.is_connect() {
        "the connection was refused".to_owned()
    } else {
        error.to_string()
    };

    DomainError::validation(format!(
        "{}: {url} could not be fetched — {reason}",
        error_codes::FETCH_FAILED
    ))
}

fn too_large(bytes: usize) -> DomainError {
    DomainError::validation(format!(
        "{}: the page is at least {bytes} bytes, past the limit of {MAX_BYTES}",
        error_codes::FILE_TOO_LARGE
    ))
}
