use crate::*;
use openbindings::{Conformant, Document, DocumentValidation, VersionRefusal};
use std::sync::Arc;

/// Publication CORS policy. Exact origins must be canonical ASCII tuple origins.
#[derive(Clone, Default, PartialEq, Eq)]
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
impl std::fmt::Debug for AllowOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => f.write_str("Any"),
            Self::Omit => f.write_str("Omit"),
            Self::Null => f.write_str("Null"),
            Self::Exact(_) => f.debug_tuple("Exact").field(&"<redacted>").finish(),
        }
    }
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
/// Later versions may add refusals, so a `match` needs a fallback arm.
#[non_exhaustive]
pub enum PublicationError {
    /// Invalid publication options, such as an unsafe CORS origin.
    Configuration(ConfigurationError),
    /// Unsupported declared version, separate from normative conformance.
    VersionRefused(VersionRefusal),
    /// Validation established a document violation; payload carries report and any document snapshot.
    NonConformant(DocumentValidation),
    /// Validation could not establish conformance; payload preserves its evidence.
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
impl std::fmt::Debug for Publication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Publication").finish_non_exhaustive()
    }
}

impl Publication {
    /// Retain an immutable publication of a proved snapshot's exact original bytes. Validate CORS configuration; no server or listener is created.
    ///
    /// ```
    /// use openbindings::validate_document;
    /// use openbindings_http_discovery::{Publication, PublicationOptions, WELL_KNOWN_PATH};
    /// let validation = validate_document(r#"{"openbindings":"0.2.0","operations":{}}"#)?;
    /// let proof = validation.conformant().expect("all normative rules established");
    /// let publication = Publication::new(&proof, PublicationOptions::default())?;
    /// let head = publication.respond("HEAD", WELL_KNOWN_PATH);
    /// assert_eq!(head.status, 200);
    /// assert!(head.body.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    /// Returns a configuration error for an invalid exact CORS origin.
    pub fn new(
        document: &Conformant<Document>,
        options: PublicationOptions,
    ) -> Result<Self, ConfigurationError> {
        validate_allow_origin(&options.allow_origin)?;
        Ok(Self {
            body: Arc::from(document.as_document().as_bytes()),
            allow_origin: options.allow_origin,
        })
    }
    /// Validate exact source bytes and create publication only on established conformance. Return distinct configuration, version, nonconformant or undetermined refusals.
    ///
    /// # Errors
    /// Returns a publication error for invalid CORS configuration, an unsupported version, or a document whose conformance is non-conformant or undetermined.
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
    /// `path` is the request's route path without query or fragment, its escapes left undecoded. A
    /// percent-encoded unreserved character matches that character (RFC 3986 section 2.3), so
    /// `/.well-known/%6Fpenbindings` is the route; a reserved escape such as `%2F` never matches
    /// `/`. Accept does not affect the default OBI representation. Authentication is external.
    pub fn respond(&self, method: &str, path: &str) -> PublicationResponse {
        let mut response = PublicationResponse {
            status: 404,
            headers: Vec::new(),
            body: Arc::from(&b""[..]),
        };
        if !names_well_known_path(path) {
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
/// Whether `path` names the well-known route. URIs differing only in a percent-encoded unreserved
/// character, with hexadecimal digits of either case, identify the same resource (RFC 3986
/// sections 2.1 and 2.3); a reserved character and its escape are not equivalent (section 2.2).
fn names_well_known_path(path: &str) -> bool {
    let mut rest = path.as_bytes();
    let mut expected = WELL_KNOWN_PATH.bytes();
    loop {
        let (byte, width) = match rest {
            [] => return expected.next().is_none(),
            [b'%', high, low, ..] => match unreserved_escape(*high, *low) {
                Some(decoded) => (decoded, 3),
                None => (b'%', 1),
            },
            [byte, ..] => (*byte, 1),
        };
        if expected.next() != Some(byte) {
            return false;
        }
        rest = &rest[width..];
    }
}
/// The RFC 3986 section 2.3 `unreserved` character a `%` triplet's two hexadecimal digits encode.
fn unreserved_escape(high: u8, low: u8) -> Option<u8> {
    let decoded = (hex_digit(high)? << 4) | hex_digit(low)?;
    (decoded.is_ascii_alphanumeric() || matches!(decoded, b'-' | b'.' | b'_' | b'~'))
        .then_some(decoded)
}
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone)]
/// Owned HTTP response data for the caller's server adapter; no I/O or listener side effect.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct PublicationResponse {
    /// HTTP status selected from route and method.
    pub status: u16,
    /// Response headers, including original-body Content-Length for HEAD.
    pub headers: Vec<Header>,
    /// Shared exact body for GET; empty for HEAD, unsupported methods and unmatched routes.
    pub body: Arc<[u8]>,
}
impl std::fmt::Debug for PublicationResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublicationResponse")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

// Validate a typed CORS choice, including an exact ASCII browser-serialized
// tuple origin. Opaque schemes, credentials and noncanonical spellings are
// refused. `Publication::new` and `from_bytes` apply it to their options.
pub(crate) fn validate_allow_origin(origin: &AllowOrigin) -> Result<(), ConfigurationError> {
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
