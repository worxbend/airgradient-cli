use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde_json::Value;
use thiserror::Error;
use url::Url;

const CURRENT_MEASURES_PATH: &str = "measures/current";
const DEFAULT_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

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

    #[error("failed to parse AirGradient measurements JSON from {url}")]
    JsonParse {
        url: Url,
        #[source]
        source: reqwest::Error,
    },
}

pub fn normalize_base_url(input: &str) -> Result<Url, DeviceError> {
    let trimmed = input.trim();
    let parse_input = if has_explicit_scheme(trimmed) {
        trimmed.to_owned()
    } else {
        format!("http://{trimmed}")
    };

    let mut url = Url::parse(&parse_input).map_err(|source| DeviceError::InvalidUrl {
        input: input.to_owned(),
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

    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);

    Ok(url)
}

pub async fn fetch_current_measures(base_url: &Url) -> Result<Value, DeviceError> {
    let client = Client::builder()
        .timeout(DEFAULT_FETCH_TIMEOUT)
        .build()
        .map_err(DeviceError::ClientBuild)?;

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

    response
        .json::<Value>()
        .await
        .map_err(|source| DeviceError::JsonParse {
            url: endpoint,
            source,
        })
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
    fn builds_exact_current_measures_endpoint() {
        let base = normalize_base_url("http://192.168.1.201/foo?ignored=true#fragment")
            .expect("URL should normalize");
        let endpoint = current_measures_url(&base);

        assert_eq!(endpoint.as_str(), "http://192.168.1.201/measures/current");
    }

    #[tokio::test]
    async fn fetch_current_measures_uses_default_timeout() {
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

        let err = fetch_current_measures(&base)
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
