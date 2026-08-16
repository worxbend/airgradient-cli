//! The network boundary: fetching measurements from a device.
//!
//! What counts as a valid device address is not decided here — that is
//! [`crate::device_url`], which this module and the config file both depend on
//! rather than on each other. This module only makes requests.
//!
//! Everything it does defensively is because the peer is a device on a local
//! network that the user named and this tool cannot vouch for. Hence a request
//! timeout, a hard cap on the response body, and no redirect following — see
//! the individual items for why each one matters.

use std::time::Duration;

use reqwest::{Client, StatusCode, redirect};
use serde_json::Value;
use thiserror::Error;
use url::Url;

use crate::device_url::DeviceUrl;

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

pub async fn fetch_current_measures_with_client(
    client: &Client,
    base_url: &DeviceUrl,
) -> Result<Value, DeviceError> {
    let endpoint = base_url.measures_endpoint();

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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

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
        let base = DeviceUrl::parse(&server.uri()).expect("URL should normalize");
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
        let base = DeviceUrl::parse(&server.uri()).expect("URL should normalize");
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
        let base = DeviceUrl::parse(&server.uri()).expect("URL should normalize");
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
        let base = DeviceUrl::parse(&server.uri()).expect("URL should normalize");
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
        let base = DeviceUrl::parse(&format!("{}/ignored?debug=true#now", server.uri()))
            .expect("URL should normalize");
        let client = Client::new();

        let payload = fetch_current_measures_with_client(&client, &base)
            .await
            .expect("request should succeed");

        assert_eq!(payload["pm02"], 7.4);
        server.verify().await;
    }
}
