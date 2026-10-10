//! Optional OpenBindings HTTP discovery companion. Core never acquires resources.
//!
//! The portable client accepts a caller's asynchronous transport. Enable `native`
//! for the reusable reqwest adapter. Publication returns ordinary response data,
//! so applications keep ownership of their HTTP server and authorization policy.
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
pub const APPLIED_COMPANION_REVISION: &str = "2f7d754dc2da374058cd517064c17e50f7d95d99";
/// Default decoded discovery-body limit: 1 MiB (1,048,576 bytes).
pub const DEFAULT_MAX_DOCUMENT_BYTES: usize = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq)]
/// Caller configuration rejected before acquisition/publication; contains stable code and fixed explanation.
pub struct ConfigurationError {
    /// Stable configuration failure identifier.
    pub code: &'static str,
    /// Human-readable explanation; branch on `code` instead.
    pub message: &'static str,
}
impl std::fmt::Display for ConfigurationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}
impl std::error::Error for ConfigurationError {}

/// Construct the discovery URL without silently discarding a resource path.
/// Host spelling is preserved. The transport remains responsible for network,
/// scheme and credential policies at each redirect and connection.
pub fn endpoint(origin: &str) -> Result<String, ConfigurationError> {
    let invalid = || ConfigurationError {
        code: "invalid-origin",
        message: "expected an absolute HTTP(S) origin without credentials, resource path, query or fragment",
    };
    let uri = fluent_uri::Uri::parse(origin).map_err(|_| invalid())?;
    let scheme = uri.scheme().as_str();
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return Err(invalid());
    }
    let authority = uri.authority().ok_or_else(invalid)?;
    if authority.host().is_empty()
        || authority.userinfo().is_some()
        || !matches!(uri.path().as_str(), "" | "/")
        || uri.query().is_some()
        || uri.fragment().is_some()
        || authority
            .port()
            .is_some_and(|p| p.as_str().is_empty() || p.as_str().parse::<u16>().is_err())
    {
        return Err(invalid());
    }
    Ok(format!(
        "{}://{}{}",
        scheme.to_ascii_lowercase(),
        authority.as_str(),
        WELL_KNOWN_PATH
    ))
}
