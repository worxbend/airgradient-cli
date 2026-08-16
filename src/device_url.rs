//! What counts as a device address, and what one can be asked for.
//!
//! This is the rule about AirGradient device URLs, and nothing else — no HTTP
//! client, no config file, no I/O. It sits below both [`crate::device`] (which
//! makes requests to one) and [`crate::config`] (which stores one) so that
//! neither has to depend on the other to agree on what a valid address is.
//!
//! Everything here is defensive because the peer is a device on a local
//! network that the user named and this tool cannot vouch for. A [`DeviceUrl`]
//! can only be built by [`DeviceUrl::parse`], so anything holding one already
//! knows it is an `http`/`https` address with no credentials and no path — the
//! rest of the program never re-checks and never has to.

use std::fmt;

use thiserror::Error;
use url::Url;

/// The single endpoint this tool talks to.
const CURRENT_MEASURES_PATH: &str = "measures/current";

/// A validated AirGradient device address: `scheme://host[:port]/`.
///
/// Construct with [`DeviceUrl::parse`]. The inner [`Url`] is private on
/// purpose — a caller that could edit it could reintroduce the path, query, or
/// credentials this type exists to have removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceUrl(Url);

#[derive(Debug, Error)]
pub enum DeviceUrlError {
    #[error("invalid device URL `{input}`")]
    Invalid {
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
}

impl DeviceUrl {
    /// Turns whatever the user typed into a bare `scheme://host[:port]/` base
    /// address.
    ///
    /// A bare host gets `http://` prepended, so `192.168.1.201` works. Anything
    /// after the host is dropped (path, query, fragment) because the only
    /// endpoint this tool talks to is the one [`DeviceUrl::measures_endpoint`]
    /// appends.
    ///
    /// URLs that embed credentials (`http://user:pass@host`) are rejected
    /// rather than stored: the address is written to the config file in
    /// plaintext and printed by `config show`, `--json`, the TUI status line,
    /// and every error message, so a password in it would end up in shell
    /// scrollback and pasted bug reports.
    pub fn parse(input: &str) -> Result<Self, DeviceUrlError> {
        let trimmed = input.trim();
        let parse_input = if has_explicit_scheme(trimmed) {
            trimmed.to_owned()
        } else {
            format!("http://{trimmed}")
        };

        let mut url = Url::parse(&parse_input).map_err(|source| DeviceUrlError::Invalid {
            input: redact_userinfo(input),
            source,
        })?;

        match url.scheme() {
            "http" | "https" => {}
            scheme => {
                return Err(DeviceUrlError::UnsupportedScheme {
                    scheme: scheme.to_owned(),
                });
            }
        }

        if !url.username().is_empty() || url.password().is_some() {
            return Err(DeviceUrlError::CredentialsInUrl);
        }

        url.set_path("");
        url.set_query(None);
        url.set_fragment(None);

        Ok(Self(url))
    }

    /// The full URL of this device's current-measurements endpoint.
    pub fn measures_endpoint(&self) -> Url {
        let mut endpoint = self.0.clone();
        endpoint
            .path_segments_mut()
            .expect("http and https URLs support path segments")
            .clear()
            .extend(CURRENT_MEASURES_PATH.split('/'));
        endpoint.set_query(None);
        endpoint.set_fragment(None);
        endpoint
    }

    /// The address as stored and displayed, with its trailing slash.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for DeviceUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
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

fn has_explicit_scheme(input: &str) -> bool {
    input
        .split_once(':')
        .is_some_and(|(scheme, rest)| !scheme.is_empty() && rest.starts_with("//"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_bare_host_to_http_base_url() {
        let url = DeviceUrl::parse("192.168.1.201").expect("URL should normalize");

        assert_eq!(url.as_str(), "http://192.168.1.201/");
    }

    #[test]
    fn normalizes_bare_host_with_port_to_http_base_url() {
        let url = DeviceUrl::parse("localhost:9925").expect("URL should normalize");

        assert_eq!(url.as_str(), "http://localhost:9925/");
    }

    #[test]
    fn accepts_http_and_https_schemes() {
        let http = DeviceUrl::parse("http://192.168.1.201").expect("URL should normalize");
        let https = DeviceUrl::parse("https://airgradient.local").expect("URL should normalize");

        assert_eq!(http.as_str(), "http://192.168.1.201/");
        assert_eq!(https.as_str(), "https://airgradient.local/");
    }

    #[test]
    fn rejects_unsupported_schemes() {
        let err = DeviceUrl::parse("ftp://192.168.1.201").expect_err("URL should be rejected");

        assert!(matches!(
            err,
            DeviceUrlError::UnsupportedScheme { scheme } if scheme == "ftp"
        ));
    }

    #[test]
    fn strips_path_query_and_fragment() {
        let url = DeviceUrl::parse("http://192.168.1.201/foo/bar?debug=true#latest")
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
            let err = DeviceUrl::parse(input).expect_err("credentials should be rejected");

            assert!(matches!(err, DeviceUrlError::CredentialsInUrl), "{input}");
        }
    }

    #[test]
    fn credential_rejection_message_does_not_leak_the_password() {
        let err = DeviceUrl::parse("http://admin:hunter2@192.168.1.201")
            .expect_err("credentials should be rejected");

        assert!(!err.to_string().contains("hunter2"));
    }

    #[test]
    fn invalid_url_error_masks_embedded_credentials() {
        let err = DeviceUrl::parse("http://admin:hunter2@:::/measures")
            .expect_err("URL should be rejected");

        let message = err.to_string();
        assert!(!message.contains("hunter2"), "{message}");
        assert!(message.contains("***@"), "{message}");
    }

    #[test]
    fn builds_exact_current_measures_endpoint() {
        let base = DeviceUrl::parse("http://192.168.1.201/foo?ignored=true#fragment")
            .expect("URL should normalize");

        assert_eq!(
            base.measures_endpoint().as_str(),
            "http://192.168.1.201/measures/current"
        );
    }

    #[test]
    fn parsing_is_idempotent_so_a_stored_url_survives_a_round_trip() {
        // The config file holds the normalized form; reading it back and
        // re-parsing must not change it, or a `config show` would disagree
        // with what was written.
        let once = DeviceUrl::parse("192.168.1.201/whatever?x=1").expect("URL should normalize");
        let twice = DeviceUrl::parse(once.as_str()).expect("normalized URL should re-parse");

        assert_eq!(once, twice);
    }
}
