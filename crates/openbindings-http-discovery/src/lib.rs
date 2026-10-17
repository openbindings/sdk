#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
mod client;
mod publication;
pub use client::*;
pub use publication::*;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native;

/// Fixed HTTP discovery route, `/.well-known/openbindings`.
pub const WELL_KNOWN_PATH: &str = "/.well-known/openbindings";
/// OpenBindings discovery media type, `application/vnd.openbindings+json`.
pub const MEDIA_TYPE: &str = "application/vnd.openbindings+json";
/// Request Accept value for the companion route, including JSON fallback.
pub const ACCEPT: &str = "application/vnd.openbindings+json, application/json";
/// Applied HTTP discovery companion version, independent of the SDK crate version.
pub const COMPANION_VERSION: &str = "0.1.0";
/// Exact specification revision defining this companion's behavior.
pub const APPLIED_COMPANION_REVISION: &str = "30416ef1ca626e58eb8c383a0c71422a68f9243f";
/// Default decoded discovery-body limit: 1 MiB (1,048,576 bytes).
pub const DEFAULT_MAX_DOCUMENT_BYTES: usize = 1 << 20;

#[derive(Clone, PartialEq, Eq)]
/// Caller configuration rejected before acquisition/publication; contains stable code and fixed explanation.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct ConfigurationError {
    /// Stable configuration failure identifier.
    pub code: &'static str,
    /// Human-readable explanation; branch on `code` instead.
    pub message: &'static str,
}
impl std::fmt::Debug for ConfigurationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = match self.code {
            "invalid-allow-origin" => "invalid-allow-origin",
            "invalid-origin" => "invalid-origin",
            "origin-policy" => "origin-policy",
            "http-client-build" => "http-client-build",
            "invalid-timeout" => "invalid-timeout",
            "invalid-byte-limit" => "invalid-byte-limit",
            _ => "<redacted>",
        };
        f.debug_struct("ConfigurationError")
            .field("code", &code)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for ConfigurationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}
impl std::error::Error for ConfigurationError {}

/// Construct and admit the normalized discovery URL. The origin must first be
/// an RFC 3986 URI, which is ASCII only, so a non-ASCII domain name is refused;
/// pass its IDNA A-label form. It is then parsed and serialized with the same
/// WHATWG parser as reqwest. HTTP is permitted only for literal IPv4 127/8 or
/// IPv6 ::1. Credentials, resource paths, query and fragment are refused before I/O.
///
/// # Errors
/// Returns a configuration error for a malformed origin, a non-HTTP(S) scheme, non-loopback HTTP, credentials, a resource path, query, or fragment.
pub fn endpoint(origin: &str) -> Result<String, ConfigurationError> {
    let invalid = || ConfigurationError {
        code: "invalid-origin",
        message: "expected an absolute HTTP(S) origin without credentials, resource path, query or fragment",
    };
    // Retain strict origin framing; do not let a forgiving URL parser erase input.
    let uri = fluent_uri::Uri::parse(origin).map_err(|_| invalid())?;
    let authority = uri.authority().ok_or_else(invalid)?;
    if authority.userinfo().is_some()
        || !matches!(uri.path().as_str(), "" | "/")
        || uri.query().is_some()
        || uri.fragment().is_some()
        || origin.contains('\\')
        || origin.bytes().any(|b| b <= b' ' || b == 127)
        || authority.port().is_some_and(|p| p.as_str().is_empty())
    {
        return Err(invalid());
    }
    let mut url = url::Url::parse(origin).map_err(|_| invalid())?;
    admit_url(&url).map_err(|_| ConfigurationError {
        code: "origin-policy",
        message: "discovery requires HTTPS except for literal IPv4 127/8 or IPv6 ::1, without credentials",
    })?;
    url.set_path(WELL_KNOWN_PATH);
    Ok(url.into())
}

pub(crate) fn admit_url(url: &url::Url) -> Result<(), &'static str> {
    if !url.username().is_empty() || url.password().is_some() || url.host().is_none() {
        return Err("discovery destination must have a host and no credentials");
    }
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.octets()[0] == 127,
        Some(url::Host::Ipv6(ip)) => ip == std::net::Ipv6Addr::LOCALHOST,
        _ => false,
    };
    if url.scheme() == "https" || (url.scheme() == "http" && loopback) {
        Ok(())
    } else {
        Err("discovery requires HTTPS except for literal IPv4 127/8 or IPv6 ::1")
    }
}

pub(crate) fn admit_redirect(previous: &url::Url, next: &url::Url) -> Result<(), &'static str> {
    if previous.scheme() == "https" && next.scheme() == "http" {
        return Err("HTTPS to HTTP discovery redirect refused");
    }
    admit_url(next)
}

/// Default whole-attempt duration in seconds, including acquisition and validation.
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 30;
/// Maximum number of redirects followed by the checked native and portable paths.
pub const MAX_REDIRECTS: usize = 10;

// One token/future identity across all asynchronous companions.
pub use openbindings_internal_cancellation::{Cancellation, CancellationToken};
