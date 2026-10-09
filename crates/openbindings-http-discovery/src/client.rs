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
pub struct Header {
    pub name: String,
    pub value: Vec<u8>,
}
impl Header {
    pub fn new(name: impl Into<String>, value: impl AsRef<[u8]>) -> Self {
        Self {
            name: name.into(),
            value: value.as_ref().into(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ResponseMetadata {
    pub requested_url: String,
    /// None when a custom transport or host cannot expose the final URL.
    pub final_url: Option<String>,
    pub status: u16,
    pub headers: Vec<Header>,
}
impl ResponseMetadata {
    pub fn header(&self, name: &str) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_slice())
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    Network,
    Timeout,
    Aborted,
    Other,
}
#[derive(Clone, Debug, Serialize)]
pub struct RequestFailure {
    pub kind: FailureKind,
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
    fn read(&mut self, output: &mut [u8]) -> impl Future<Output = Result<usize, RequestFailure>>;
    /// Optional bounded cleanup for connection reuse. The default closes on drop.
    /// Implementations must bound their own cleanup time and bytes; failures must
    /// never replace an already observed HTTP status.
    fn discard_status(&mut self) -> impl Future<Output = ()> {
        std::future::ready(())
    }
}
pub struct HttpResponse<B> {
    pub final_url: Option<String>,
    pub status: u16,
    pub headers: Vec<Header>,
    pub body: B,
}
#[derive(Clone, Debug)]
pub struct DiscoveryRequest {
    pub url: String,
    pub accept: &'static str,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ClientOptions {
    /// Bound on decoded bytes delivered by the transport. Zero selects 1 MiB;
    /// negative values are configuration errors. At most one extra byte is read.
    pub max_document_bytes: i64,
}
impl ClientOptions {
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
pub enum DiscoveryOutcome {
    Found { document: ValidatedDocument },
    NonConformant { assessment: DocumentAssessment },
    Undetermined { assessment: DocumentAssessment },
    VersionRefused { refusal: VersionRefusal },
    Absent,
    Gated,
    HttpStatus,
    BodyLimit { limit: usize },
    Cancelled,
    TransportFailure { failure: RequestFailure },
    BodyFailure { failure: RequestFailure },
}
#[derive(Clone)]
pub struct DiscoveryResult {
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
