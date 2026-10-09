use crate::*;
use openbindings::{DocumentAssessment, ValidatedDocument, VersionRefusal};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct PublicationOptions {
    /// Empty omits CORS; `*`, `null`, or one ASCII origin is emitted unchanged.
    /// Credentialed CORS and preflight policy belong to application middleware.
    pub allow_origin: String,
}
#[derive(Clone, Debug)]
pub enum PublicationError {
    Configuration(ConfigurationError),
    VersionRefused(VersionRefusal),
    NonConformant(DocumentAssessment),
    Undetermined(DocumentAssessment),
}
impl std::fmt::Display for PublicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Configuration(_) => "invalid publication configuration",
            Self::VersionRefused(_) => "publication version is not supported",
            Self::NonConformant(_) => "publication requires a conformant document",
            Self::Undetermined(_) => "publication conformance could not be established",
        })
    }
}
impl std::error::Error for PublicationError {}
/// Immutable, concurrently shareable publication snapshot. It owns exact bytes.
#[derive(Clone)]
pub struct Publication {
    body: Arc<[u8]>,
    allow_origin: String,
}
impl Publication {
    pub fn new(
        document: &ValidatedDocument,
        options: PublicationOptions,
    ) -> Result<Self, ConfigurationError> {
        validate_allow_origin(&options.allow_origin)?;
        Ok(Self {
            body: Arc::from(document.parsed().original_bytes()),
            allow_origin: options.allow_origin,
        })
    }
    pub fn from_bytes(
        bytes: impl AsRef<[u8]>,
        options: PublicationOptions,
    ) -> Result<Self, PublicationError> {
        validate_allow_origin(&options.allow_origin).map_err(PublicationError::Configuration)?;
        match assess_body(bytes.as_ref()) {
            DiscoveryOutcome::Found { document } => {
                Self::new(&document, options).map_err(PublicationError::Configuration)
            }
            DiscoveryOutcome::VersionRefused { refusal } => {
                Err(PublicationError::VersionRefused(refusal))
            }
            DiscoveryOutcome::NonConformant { assessment } => {
                Err(PublicationError::NonConformant(assessment))
            }
            DiscoveryOutcome::Undetermined { assessment } => {
                Err(PublicationError::Undetermined(assessment))
            }
            _ => unreachable!("body assessment returns only document outcomes"),
        }
    }
    /// `path` is the host's decoded route path, excluding query/fragment. Accept
    /// does not affect the default OBI representation. Authentication is external.
    pub fn respond(&self, method: &str, path: &str) -> PublicationResponse {
        let mut response = PublicationResponse {
            status: 404,
            headers: Vec::new(),
            body: Arc::from(&b""[..]),
        };
        if path != WELL_KNOWN_PATH {
            return response;
        }
        if !self.allow_origin.is_empty() {
            response.headers.push(Header::new(
                "access-control-allow-origin",
                &self.allow_origin,
            ));
        }
        if !matches!(method, "GET" | "HEAD") {
            response.status = 405;
            response.headers.push(Header::new("allow", "GET, HEAD"));
            return response;
        }
        response.status = 200;
        response
            .headers
            .push(Header::new("content-type", MEDIA_TYPE));
        response
            .headers
            .push(Header::new("content-length", self.body.len().to_string()));
        if method == "GET" {
            response.body = self.body.clone();
        }
        response
    }
}
#[derive(Clone)]
pub struct PublicationResponse {
    pub status: u16,
    pub headers: Vec<Header>,
    pub body: Arc<[u8]>,
}

pub fn validate_allow_origin(origin: &str) -> Result<(), ConfigurationError> {
    let invalid = || ConfigurationError {
        code: "invalid-allow-origin",
        message: "allow_origin must be empty, *, null, or one ASCII origin without credentials, path, query or fragment",
    };
    if matches!(origin, "" | "*" | "null") {
        return Ok(());
    }
    if origin.bytes().any(|b| b <= b' ' || b >= 0x7f) {
        return Err(invalid());
    }
    let uri = fluent_uri::Uri::parse(origin).map_err(|_| invalid())?;
    let authority = uri.authority().ok_or_else(invalid)?;
    let host = authority.host();
    if host.is_empty()
        || authority.userinfo().is_some()
        || !uri.path().is_empty()
        || uri.query().is_some()
        || uri.fragment().is_some()
        || host.as_bytes().iter().any(|b| b",%*\\<>\"".contains(b))
        || authority
            .port()
            .is_some_and(|p| p.as_str().is_empty() || p.as_str().parse::<u16>().is_err())
    {
        return Err(invalid());
    }
    if host.starts_with('[') {
        if !host.ends_with(']')
            || host[1..host.len() - 1]
                .parse::<std::net::Ipv6Addr>()
                .is_err()
        {
            return Err(invalid());
        }
    } else if host.contains(['[', ']', ':']) {
        return Err(invalid());
    }
    Ok(())
}
