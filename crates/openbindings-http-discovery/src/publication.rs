use crate::*;
use openbindings::{DocumentAssessment, ValidatedDocument, VersionRefusal};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
/// Publication header policy. Authentication, credentials and preflight handling remain the caller's responsibility.
pub struct PublicationOptions {
    /// Empty omits CORS; `*`, `null`, or one ASCII origin is emitted unchanged.
    /// Credentialed CORS and preflight policy belong to application middleware.
    pub allow_origin: String,
}
#[derive(Clone, Debug)]
/// Publication setup refusal; no listener is started and no unproved body is published.
pub enum PublicationError {
    /// Invalid publication options, such as an unsafe CORS origin.
    Configuration(ConfigurationError),
    /// Unsupported declared version, separate from normative conformance.
    VersionRefused(VersionRefusal),
    /// Assessment established a document violation; payload carries report and any parsed snapshot.
    NonConformant(DocumentAssessment),
    /// Assessment could not establish conformance; payload preserves its evidence.
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
    /// Retain an immutable publication of a proved snapshot's exact original bytes. Validate CORS configuration; no server or listener is created.
    ///
    /// ```
    /// use openbindings::assess_document;
    /// use openbindings_http_discovery::{Publication, PublicationOptions, WELL_KNOWN_PATH};
    /// let assessment = assess_document(r#"{"openbindings":"0.2.0","operations":{}}"#)?;
    /// let proof = assessment.validated().expect("all normative rules established");
    /// let publication = Publication::new(&proof, PublicationOptions::default())?;
    /// let head = publication.respond("HEAD", WELL_KNOWN_PATH);
    /// assert_eq!(head.status, 200);
    /// assert!(head.body.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
    /// Assess exact source bytes and create publication only on established conformance. Return distinct configuration, version, nonconformant or undetermined refusals.
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
/// Owned HTTP response data for the caller's server adapter; no I/O or listener side effect.
pub struct PublicationResponse {
    /// HTTP status selected from route and method.
    pub status: u16,
    /// Response headers, including original-body Content-Length for HEAD.
    pub headers: Vec<Header>,
    /// Shared exact body for GET; empty for HEAD, unsupported methods and unmatched routes.
    pub body: Arc<[u8]>,
}

/// Validate empty (omit), `*`, `null`, or one ASCII HTTP(S) origin without credentials, path, query, fragment or controls. The accepted spelling is emitted unchanged.
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
