use crate::*;
use openbindings::{DocumentValidation, ValidatedDocument, VersionRefusal};
use std::sync::Arc;

/// Publication CORS policy. Exact origins must be canonical ASCII tuple origins.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AllowOrigin {
    /// Emit the wildcard origin `*` (the default).
    #[default]
    Any,
    /// Omit the Access-Control-Allow-Origin header.
    Omit,
    /// Emit the opaque-origin spelling `null`.
    Null,
    /// Emit a canonical ASCII serialized tuple origin.
    Exact(String),
}

#[derive(Clone, Debug)]
/// Publication header policy. Authentication, credentials and preflight handling remain the caller's responsibility.
pub struct PublicationOptions {
    /// Defaults to any origin. Credentialed CORS and preflight policy belong to application middleware.
    pub allow_origin: AllowOrigin,
}
impl Default for PublicationOptions {
    fn default() -> Self {
        Self {
            allow_origin: AllowOrigin::Any,
        }
    }
}

#[derive(Clone, Debug)]
/// Publication setup refusal; no listener is started and no unproved body is published.
pub enum PublicationError {
    /// Invalid publication options, such as an unsafe CORS origin.
    Configuration(ConfigurationError),
    /// Unsupported declared version, separate from normative conformance.
    VersionRefused(VersionRefusal),
    /// Assessment established a document violation; payload carries report and any parsed snapshot.
    NonConformant(DocumentValidation),
    /// Assessment could not establish conformance; payload preserves its evidence.
    Undetermined(DocumentValidation),
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
    allow_origin: AllowOrigin,
}
impl Publication {
    /// Retain an immutable publication of a proved snapshot's exact original bytes. Validate CORS configuration; no server or listener is created.
    ///
    /// ```
    /// use openbindings::validate_document;
    /// use openbindings_http_discovery::{Publication, PublicationOptions, WELL_KNOWN_PATH};
    /// let validation = validate_document(r#"{"openbindings":"0.2.0","operations":{}}"#)?;
    /// let proof = validation.validated().expect("all normative rules established");
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
        match validate_body(bytes.as_ref()) {
            DiscoveryOutcome::Found { document } => {
                Self::new(&document, options).map_err(PublicationError::Configuration)
            }
            DiscoveryOutcome::VersionRefused { refusal } => {
                Err(PublicationError::VersionRefused(refusal))
            }
            DiscoveryOutcome::NonConformant { validation } => {
                Err(PublicationError::NonConformant(validation))
            }
            DiscoveryOutcome::Undetermined { validation } => {
                Err(PublicationError::Undetermined(validation))
            }
            _ => unreachable!("body validation returns only document outcomes"),
        }
    }
    /// `path` is the exact route path, excluding query/fragment. Do not decode reserved escapes. Accept
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
        if let Some(origin) = match &self.allow_origin {
            AllowOrigin::Any => Some("*"),
            AllowOrigin::Omit => None,
            AllowOrigin::Null => Some("null"),
            AllowOrigin::Exact(origin) => Some(origin.as_str()),
        } {
            response
                .headers
                .push(Header::new("access-control-allow-origin", origin));
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

/// Validate a typed CORS choice, including an exact ASCII browser-serialized
/// tuple origin. Opaque schemes, credentials and noncanonical spellings are refused.
pub fn validate_allow_origin(origin: &AllowOrigin) -> Result<(), ConfigurationError> {
    let invalid = || ConfigurationError {
        code: "invalid-allow-origin",
        message: "exact allow_origin must be a canonical ASCII serialized tuple origin",
    };
    let AllowOrigin::Exact(origin) = origin else {
        return Ok(());
    };
    if origin.bytes().any(|b| b <= b' ' || b >= 0x7f)
        || origin.contains([',', '*', '\\', '<', '>', '"'])
    {
        return Err(invalid());
    }
    let parsed = url::Url::parse(origin).map_err(|_| invalid())?;
    if matches!(parsed.origin(), url::Origin::Opaque(_))
        || parsed.origin().ascii_serialization() != *origin
    {
        return Err(invalid());
    }
    Ok(())
}
