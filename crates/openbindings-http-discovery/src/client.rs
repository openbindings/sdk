use crate::*;
use openbindings::{
    Conformance, DocumentAssessment, ValidatedDocument, VersionRefusal, WorkControl,
    assess_document,
};
use serde::Serialize;
use std::{
    future::{Future, poll_fn},
    pin::pin,
    sync::Arc,
    task::Poll,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
/// Owned HTTP header name and uninterpreted value bytes, preserving non-UTF-8 values.
pub struct Header {
    /// Header name; lookup uses ASCII case-insensitive comparison.
    pub name: String,
    /// Raw field value bytes; consumers choose an appropriate text-decoding/display policy.
    pub value: Vec<u8>,
}
impl Header {
    /// Copy a header name and byte value into owned response metadata.
    pub fn new(name: impl Into<String>, value: impl AsRef<[u8]>) -> Self {
        Self {
            name: name.into(),
            value: value.as_ref().into(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
/// Response facts retained independently from body acquisition or document assessment.
pub struct ResponseMetadata {
    /// Constructed discovery URL sent to the caller's transport.
    pub requested_url: String,
    /// None when a custom transport or host cannot expose the final URL.
    pub final_url: Option<String>,
    /// Observed HTTP status code; only 200 bodies enter assessment.
    pub status: u16,
    /// Owned observed headers, retaining repeated names and raw values.
    pub headers: Vec<Header>,
}
impl ResponseMetadata {
    /// Borrow the first case-insensitive matching header value, or `None`; does not combine repeated fields.
    pub fn header(&self, name: &str) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_slice())
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
/// Transport-provided failure category; distinct from document evidence and HTTP response status.
pub enum FailureKind {
    /// The transport could not complete a network operation.
    Network,
    /// A transport-owned deadline expired.
    Timeout,
    /// The transport reports cancellation or abortion.
    Aborted,
    /// The transport cannot classify the failure more specifically.
    Other,
}
#[derive(Clone, Debug, Serialize)]
/// Caller-transport failure during request or body reading. Its message is supplied by the adapter, not normative evidence.
pub struct RequestFailure {
    /// Transport failure category.
    pub kind: FailureKind,
    /// Adapter-provided explanation; adapters should remove credentials, URLs and source data from default diagnostics.
    pub message: String,
}
impl std::fmt::Display for RequestFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for RequestFailure {}

/// A decoded response body. `read` must not write outside `output` and returns
/// zero only at EOF. Dropping the body must release/cancel host response state.
/// Futures need not be Send, permitting local browser/Worker transports.
pub trait ResponseBody {
    /// Read at most `output.len()` decoded bytes and return their count; zero means EOF. Do not return more than the buffer length. Dropping the body must release/cancel acquisition; futures need not be `Send`.
    fn read(&mut self, output: &mut [u8]) -> impl Future<Output = Result<usize, RequestFailure>>;
    /// Optional bounded cleanup for connection reuse. The default closes on drop.
    /// Implementations must bound their own cleanup time and bytes; failures must
    /// never replace an already observed HTTP status.
    fn discard_status(&mut self) -> impl Future<Output = ()> {
        std::future::ready(())
    }
}
/// Response supplied by the caller transport, before bounded body reading. The transport owns redirects, credentials, TLS and timeouts.
pub struct HttpResponse<B> {
    /// Final URL when known, for metadata only; never an implicit schema base or acquisition permission.
    pub final_url: Option<String>,
    /// Observed HTTP response status.
    pub status: u16,
    /// Owned response headers, preserving repeated fields and raw bytes.
    pub headers: Vec<Header>,
    /// Decoded-body stream whose drop releases transport resources.
    pub body: B,
}
#[derive(Clone, Debug)]
/// One finite request constructed by the portable discovery client; the callback performs acquisition.
pub struct DiscoveryRequest {
    /// Absolute discovery endpoint derived from the caller's HTTP(S) origin.
    pub url: String,
    /// Companion Accept header value to send with the request.
    pub accept: &'static str,
}

#[derive(Clone, Copy, Debug, Default)]
/// Portable discovery configuration. Default uses a 1 MiB decoded-body limit; zero selects the default and negative values are refused.
pub struct ClientOptions {
    /// Bound on decoded bytes delivered by the transport. Zero selects 1 MiB;
    /// negative values are configuration errors. At most one extra byte is read.
    pub max_document_bytes: i64,
}
impl ClientOptions {
    /// Resolve the configured decoded-byte limit, reserving capacity for a one-byte over-limit sentinel. Reject negative or unrepresentable limits before transport work.
    pub fn byte_limit(&self) -> Result<usize, ConfigurationError> {
        if self.max_document_bytes == 0 {
            return Ok(DEFAULT_MAX_DOCUMENT_BYTES);
        }
        usize::try_from(self.max_document_bytes).ok().filter(|n| *n < usize::MAX)
            .ok_or(ConfigurationError { code: "invalid-byte-limit", message: "maximum document bytes must be nonnegative and fit the host's address space with one sentinel byte" })
    }
    /// Retrieve and assess one document. The SDK stores no response cache and
    /// never treats the acquisition URL as a schema base URI.
    pub async fn discover_with<B, F, T>(
        &self,
        origin: &str,
        control: &WorkControl,
        transport: T,
    ) -> Result<DiscoveryResult, ConfigurationError>
    where
        B: ResponseBody,
        F: Future<Output = Result<HttpResponse<B>, RequestFailure>>,
        T: FnOnce(DiscoveryRequest) -> F,
    {
        let limit = self.byte_limit()?;
        let requested_url = endpoint(origin)?;
        let mut result = DiscoveryResult {
            metadata: None,
            body: None,
            outcome: DiscoveryOutcome::Cancelled,
        };
        if control.is_cancelled() {
            return Ok(result);
        }
        let response = until_cancelled(
            control,
            transport(DiscoveryRequest {
                url: requested_url.clone(),
                accept: ACCEPT,
            }),
        )
        .await;
        let mut response = match response {
            None => return Ok(result),
            Some(Err(failure)) => {
                result.outcome = DiscoveryOutcome::TransportFailure { failure };
                return Ok(result);
            }
            Some(Ok(response)) => response,
        };
        result.metadata = Some(ResponseMetadata {
            requested_url,
            final_url: response.final_url.take(),
            status: response.status,
            headers: std::mem::take(&mut response.headers),
        });
        if response.status != 200 {
            result.outcome = match response.status {
                404 => DiscoveryOutcome::Absent,
                401 | 403 => DiscoveryOutcome::Gated,
                _ => DiscoveryOutcome::HttpStatus,
            };
            until_cancelled(control, response.body.discard_status()).await;
            return Ok(result);
        }
        let mut body = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let remaining = (limit + 1 - body.len()).min(buffer.len());
            match until_cancelled(control, response.body.read(&mut buffer[..remaining])).await {
                None => return Ok(result),
                Some(Err(failure)) => {
                    result.outcome = DiscoveryOutcome::BodyFailure { failure };
                    return Ok(result);
                }
                Some(Ok(0)) => break,
                Some(Ok(n)) if n <= remaining => body.extend_from_slice(&buffer[..n]),
                Some(Ok(_)) => {
                    result.outcome = DiscoveryOutcome::BodyFailure {
                        failure: RequestFailure {
                            kind: FailureKind::Other,
                            message: "transport returned more bytes than the provided buffer"
                                .into(),
                        },
                    };
                    return Ok(result);
                }
            }
            if body.len() > limit {
                result.outcome = DiscoveryOutcome::BodyLimit { limit };
                return Ok(result);
            }
        }
        result.body = Some(Arc::from(body));
        if control.is_cancelled() {
            return Ok(result);
        }
        result.outcome = assess_body(result.body.as_deref().unwrap());
        if control.is_cancelled() {
            result.outcome = DiscoveryOutcome::Cancelled;
        }
        Ok(result)
    }
}
pub(crate) async fn until_cancelled<F: Future>(
    control: &WorkControl,
    future: F,
) -> Option<F::Output> {
    let mut future = pin!(future);
    let mut cancelled = pin!(control.cancelled());
    poll_fn(|cx| {
        if cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        future.as_mut().poll(cx).map(Some)
    })
    .await
}
#[derive(Clone, Debug)]
/// Acquisition and assessment partition. Only `Found` carries normative proof; HTTP status, refusal and incomplete work remain distinct.
pub enum DiscoveryOutcome {
    /// A complete bounded 200 body established normative document conformance.
    Found {
        #[doc = "Retained immutable proof for the exact acquired body."]
        document: ValidatedDocument,
    },
    /// A complete 200 body established at least one normative violation, including invalid JSON.
    NonConformant {
        #[doc = "Report and parsed snapshot when input was admitted."]
        assessment: DocumentAssessment,
    },
    /// A complete 200 body could not be proved conformant or nonconformant within supported limits/capabilities.
    Undetermined {
        #[doc = "Report preserving inconclusive evidence and any admitted snapshot."]
        assessment: DocumentAssessment,
    },
    /// A complete 200 body declared a well-formed unsupported version.
    VersionRefused {
        #[doc = "Declared/supported version information, not conformance evidence."]
        refusal: VersionRefusal,
    },
    /// HTTP 404; the only status interpreted as absence.
    Absent,
    /// HTTP 401 or 403; authorization is required or denied.
    Gated,
    /// Any observed status other than 200, 401, 403 or 404; inspect response metadata.
    HttpStatus,
    /// The decoded 200 body exceeded the configured byte limit; no partial body is published as a document.
    BodyLimit {
        #[doc = "Effective decoded-byte limit, in bytes."]
        limit: usize,
    },
    /// Caller cancellation was observed during acquisition or at assessment boundaries.
    Cancelled,
    /// The request failed before response metadata was available.
    TransportFailure {
        #[doc = "Transport classification and adapter-owned explanation."]
        failure: RequestFailure,
    },
    /// The response was obtained but decoded-body reading failed; response metadata remains available.
    BodyFailure {
        #[doc = "Body-reader classification and adapter-owned explanation."]
        failure: RequestFailure,
    },
}
#[derive(Clone)]
/// Finite discovery receipt: semantic outcome, observed response metadata and complete bounded body when available. No response cache or server is created.
pub struct DiscoveryResult {
    /// Distinct acquisition or assessment outcome; do not collapse absence, gating and inability into invalidity.
    pub outcome: DiscoveryOutcome,
    /// Present once a response has been received, including body failures.
    pub metadata: Option<ResponseMetadata>,
    /// Exact complete bounded 200 bytes, including invalid/refused documents.
    pub body: Option<Arc<[u8]>>,
}
impl std::fmt::Debug for DiscoveryResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscoveryResult")
            .field("outcome", &self.outcome)
            .field("metadata", &self.metadata)
            .field("body_bytes", &self.body.as_ref().map(|b| b.len()))
            .finish()
    }
}
/// Classify a complete bounded body. Acquisition metadata is deliberately absent.
pub fn assess_body(body: &[u8]) -> DiscoveryOutcome {
    match assess_document(body) {
        Err(refusal) => DiscoveryOutcome::VersionRefused { refusal },
        Ok(assessment) => match assessment.report().conclusion {
            Conformance::Conformant => DiscoveryOutcome::Found {
                document: assessment
                    .validated()
                    .expect("conformant assessment has a document"),
            },
            Conformance::NonConformant => DiscoveryOutcome::NonConformant { assessment },
            Conformance::Undetermined => DiscoveryOutcome::Undetermined { assessment },
        },
    }
}
