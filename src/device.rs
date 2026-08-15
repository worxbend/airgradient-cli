//! The network boundary: turning user input into a device URL, and fetching
//! measurements from it.
//!
//! Everything this module does defensively is because the peer is a device on
//! a local network that the user named and this tool cannot vouch for. Hence a
//! request timeout, a hard cap on the response body, no redirect following,
//! and a normalized URL that refuses to carry credentials — see the individual
//! items for why each one matters.

use std::time::Duration;

use reqwest::{Client, StatusCode, redirect};
use serde_json::Value;
use thiserror::Error;
use url::Url;

const CURRENT_MEASURES_PATH: &str = "measures/current";
const DEFAULT_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest `/measures/current` body we are willing to hold in memory.
///
/// A real device answers with a JSON object of a few hundred bytes. The cap
/// exists so a broken or hostile responder cannot make the CLI buffer an
/// unbounded body: the request timeout limits how *long* a transfer may take,
/// which on a fast LAN is still hundreds of megabytes.
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchSettings {
    timeout: Duration,
}

impl FetchSettings {
    pub const fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    pub const fn timeout(self) -> Duration {
        self.timeout
    }

    /// Builds the HTTP client used for device requests.
    ///
    /// Redirects are disabled on purpose. The tool talks to exactly one
    /// endpoint on a device the user named; following a `302 Location:` would
    /// let that device point the CLI at any other host on the user's network,
    /// which turns this into a request-forgery tool. A redirect now surfaces as
    /// [`DeviceError::NonSuccessStatus`] instead.
    pub fn client(self) -> Result<Client, DeviceError> {
        Client::builder()
            .timeout(self.timeout)
            .redirect(redirect::Policy::none())
            .build()
            .map_err(DeviceError::ClientBuild)
    }
}

impl Default for FetchSettings {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_FETCH_TIMEOUT,
        }
    }
}

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("invalid device URL `{input}`")]
    InvalidUrl {
        input: String,
        #[source]
        source: url::ParseError,
    },

    #[error("unsupported device URL scheme `{scheme}`; expected http or https")]
    UnsupportedScheme { scheme: String },

    #[error(
        "device URL must not embed credentials; remove the `user:password@` part before the host"
    )]
    CredentialsInUrl,

    #[error("failed to build HTTP client")]
    ClientBuild(#[source] reqwest::Error),

    #[error("failed to request AirGradient measurements from {url}")]
    Request {
        url: Url,
        #[source]
        source: reqwest::Error,
    },

    #[error("AirGradient device returned non-success status {status} from {url}")]
    NonSuccessStatus { url: Url, status: StatusCode },

    #[error("AirGradient device at {url} sent more than {limit} bytes of measurements")]
    ResponseTooLarge { url: Url, limit: usize },

    #[error("failed to parse AirGradient measurements JSON from {url}")]
    JsonParse {
        url: Url,
        #[source]
        source: serde_json::Error,
    },
}

/// Turns whatever the user typed into a bare `scheme://host[:port]/` base URL.
///
/// Everything after the host is dropped (path, query, fragment) because the only
/// endpoint this tool talks to is appended by [`current_measures_url`]. URLs that
/// embed credentials (`http://user:pass@host`) are rejected rather than stored:
/// the normalized URL is written to the config file in plaintext and printed by
/// `config show`, `--json`, the TUI status line, and every error message, so a
/// password in it would end up in shell scrollback and pasted bug reports.
pub fn normalize_base_url(input: &str) -> Result<Url, DeviceError> {
    let trimmed = input.trim();
    let parse_input = if has_explicit_scheme(trimmed) {
        trimmed.to_owned()
    } else {
        format!("http://{trimmed}")
    };

    let mut url = Url::parse(&parse_input).map_err(|source| DeviceError::InvalidUrl {
        input: redact_userinfo(input),
        source,
    })?;

    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(DeviceError::UnsupportedScheme {
                scheme: scheme.to_owned(),
            });
        }
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(DeviceError::CredentialsInUrl);
    }

    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);

    Ok(url)
}

/// Masks a `user:password@` prefix so a rejected URL can be echoed back safely.
///
/// `Url::parse` failed for this string, so we cannot use the parser to find the
/// userinfo. Everything between the scheme separator and the last `@` of the
/// authority is replaced with `***`; strings without an `@` are returned as-is.
fn redact_userinfo(input: &str) -> String {
    let (prefix, authority_and_rest) = match input.find("//") {
        Some(index) => input.split_at(index + 2),
        None => ("", input),
    };
    let authority_end = authority_and_rest
        .find('/')
        .unwrap_or(authority_and_rest.len());
    let (authority, rest) = authority_and_rest.split_at(authority_end);

    match authority.rfind('@') {
        Some(index) => format!("{prefix}***@{}{rest}", &authority[index + 1..]),
        None => input.to_owned(),
    }
}

pub async fn fetch_current_measures(base_url: &Url) -> Result<Value, DeviceError> {
    let client = FetchSettings::default().client()?;

    fetch_current_measures_with_client(&client, base_url).await
}

pub async fn fetch_current_measures_with_timeout(
    base_url: &Url,
    timeout: Duration,
) -> Result<Value, DeviceError> {
    let client = FetchSettings::new(timeout).client()?;

    fetch_current_measures_with_client(&client, base_url).await
}

pub async fn fetch_current_measures_with_client(
    client: &Client,
    base_url: &Url,
) -> Result<Value, DeviceError> {
    let endpoint = current_measures_url(base_url);

    let response = client
        .get(endpoint.clone())
        .send()
        .await
        .map_err(|source| DeviceError::Request {
            url: endpoint.clone(),
            source,
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(DeviceError::NonSuccessStatus {
            url: endpoint,
            status,
        });
    }

    let body = read_body_capped(response, &endpoint).await?;

    serde_json::from_slice(&body).map_err(|source| DeviceError::JsonParse {
        url: endpoint,
        source,
    })
}

/// Reads a response body into memory, refusing to grow past [`MAX_RESPONSE_BYTES`].
///
/// `Response::json`/`Response::bytes` buffer whatever the peer sends, so they
/// are not usable here. This reads the body one chunk at a time and bails out
/// as soon as the total would exceed the cap; the advertised `Content-Length`,
/// when present, lets us refuse before reading anything at all.
async fn read_body_capped(
    mut response: reqwest::Response,
    endpoint: &Url,
) -> Result<Vec<u8>, DeviceError> {
    let too_large = || DeviceError::ResponseTooLarge {
        url: endpoint.clone(),
        limit: MAX_RESPONSE_BYTES,
    };

    if response
        .content_length()
        .is_some_and(|length| u64::try_from(MAX_RESPONSE_BYTES).is_ok_and(|limit| length > limit))
    {
        return Err(too_large());
    }

    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|source| DeviceError::Request {
            url: endpoint.clone(),
            source,
        })?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(too_large());
        }

        body.extend_from_slice(&chunk);
    }

    Ok(body)
}

pub fn current_measures_url(base_url: &Url) -> Url {
    let mut endpoint = base_url.clone();
    endpoint
        .path_segments_mut()
        .expect("http and https URLs support path segments")
        .clear()
        .extend(CURRENT_MEASURES_PATH.split('/'));
    endpoint.set_query(None);
    endpoint.set_fragment(None);
    endpoint
}

fn has_explicit_scheme(input: &str) -> bool {
    input
        .split_once(':')
        .is_some_and(|(scheme, rest)| !scheme.is_empty() && rest.starts_with("//"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[test]
    fn normalizes_bare_host_to_http_base_url() {
        let url = normalize_base_url("192.168.1.201").expect("URL should normalize");

        assert_eq!(url.as_str(), "http://192.168.1.201/");
    }

    #[test]
    fn normalizes_bare_host_with_port_to_http_base_url() {
        let url = normalize_base_url("localhost:9925").expect("URL should normalize");

        assert_eq!(url.as_str(), "http://localhost:9925/");
    }

    #[test]
    fn accepts_http_and_https_schemes() {
        let http = normalize_base_url("http://192.168.1.201").expect("URL should normalize");
        let https = normalize_base_url("https://airgradient.local").expect("URL should normalize");

        assert_eq!(http.as_str(), "http://192.168.1.201/");
        assert_eq!(https.as_str(), "https://airgradient.local/");
    }

    #[test]
    fn rejects_unsupported_schemes() {
        let err = normalize_base_url("ftp://192.168.1.201").expect_err("URL should be rejected");

        assert!(matches!(
            err,
            DeviceError::UnsupportedScheme { scheme } if scheme == "ftp"
        ));
    }

    #[test]
    fn strips_path_query_and_fragment() {
        let url = normalize_base_url("http://192.168.1.201/foo/bar?debug=true#latest")
            .expect("URL should normalize");

        assert_eq!(url.as_str(), "http://192.168.1.201/");
    }

    #[test]
    fn rejects_urls_that_embed_credentials() {
        for input in [
            "http://admin:hunter2@192.168.1.201",
            "https://admin@airgradient.local",
            "admin:hunter2@192.168.1.201",
        ] {
            let err = normalize_base_url(input).expect_err("credentials should be rejected");

            assert!(matches!(err, DeviceError::CredentialsInUrl), "{input}");
        }
    }

    #[test]
    fn credential_rejection_message_does_not_leak_the_password() {
        let err = normalize_base_url("http://admin:hunter2@192.168.1.201")
            .expect_err("credentials should be rejected");

        assert!(!err.to_string().contains("hunter2"));
    }

    #[test]
    fn invalid_url_error_masks_embedded_credentials() {
        let err = normalize_base_url("http://admin:hunter2@:::/measures")
            .expect_err("URL should be rejected");

        let message = err.to_string();
        assert!(!message.contains("hunter2"), "{message}");
        assert!(message.contains("***@"), "{message}");
    }

    #[test]
    fn builds_exact_current_measures_endpoint() {
        let base = normalize_base_url("http://192.168.1.201/foo?ignored=true#fragment")
            .expect("URL should normalize");
        let endpoint = current_measures_url(&base);

        assert_eq!(endpoint.as_str(), "http://192.168.1.201/measures/current");
    }

    #[tokio::test]
    async fn fetch_settings_client_uses_default_timeout() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/measures/current"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(DEFAULT_FETCH_TIMEOUT + Duration::from_secs(1)),
            )
            .mount(&server)
            .await;
        let base = normalize_base_url(&server.uri()).expect("URL should normalize");
        let client = FetchSettings::default()
            .client()
            .expect("default client should build");

        let err = fetch_current_measures_with_client(&client, &base)
            .await
            .expect_err("request should time out");

        assert!(matches!(
            err,
            DeviceError::Request { source, .. } if source.is_timeout()
        ));
    }

    #[tokio::test]
    async fn fetch_current_measures_with_client_uses_provided_client() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/measures/current"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(100))
                    .set_body_json(serde_json::json!({ "pm02": 7.4 })),
            )
            .mount(&server)
            .await;
        let base = normalize_base_url(&server.uri()).expect("URL should normalize");
        let client = Client::builder()
            .timeout(Duration::from_millis(25))
            .build()
            .expect("client should build");

        let err = fetch_current_measures_with_client(&client, &base)
            .await
            .expect_err("provided client timeout should be used");

        assert!(matches!(
            err,
            DeviceError::Request { source, .. } if source.is_timeout()
        ));
    }

    #[tokio::test]
    async fn rejects_bodies_larger_than_the_cap() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/measures/current"))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(vec![b'x'; MAX_RESPONSE_BYTES + 1]),
            )
            .mount(&server)
            .await;
        let base = normalize_base_url(&server.uri()).expect("URL should normalize");
        let client = FetchSettings::default()
            .client()
            .expect("client should build");

        let err = fetch_current_measures_with_client(&client, &base)
            .await
            .expect_err("oversized body should be rejected");

        assert!(
            matches!(err, DeviceError::ResponseTooLarge { .. }),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn does_not_follow_redirects_away_from_the_device() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/measures/current"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", "http://10.0.0.5:8080/admin"),
            )
            .mount(&server)
            .await;
        let base = normalize_base_url(&server.uri()).expect("URL should normalize");
        let client = FetchSettings::default()
            .client()
            .expect("client should build");

        let err = fetch_current_measures_with_client(&client, &base)
            .await
            .expect_err("redirect should not be followed");

        assert!(
            matches!(err, DeviceError::NonSuccessStatus { status, .. } if status == StatusCode::FOUND),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn fetch_current_measures_with_client_fetches_current_measures_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/measures/current"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "pm02": 7.4
            })))
            .expect(1)
            .mount(&server)
            .await;
        let base = normalize_base_url(&format!("{}/ignored?debug=true#now", server.uri()))
            .expect("URL should normalize");
        let client = Client::new();

        let payload = fetch_current_measures_with_client(&client, &base)
            .await
            .expect("request should succeed");

        assert_eq!(payload["pm02"], 7.4);
        server.verify().await;
    }
}
