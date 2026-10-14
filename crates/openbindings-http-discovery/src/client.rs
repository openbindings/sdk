use crate::CancellationToken;
use crate::*;
use openbindings::{
    Conformance, DocumentValidation, ValidatedDocument, VersionRefusal, validate_document,
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
/// Response facts retained independently from body acquisition or document validation.
pub struct ResponseMetadata {
    /// Constructed discovery URL sent to the caller's transport.
    pub requested_url: String,
    /// None when a custom transport or host cannot expose the final URL.
    pub final_url: Option<String>,
    /// Observed HTTP status code; only 200 bodies enter validation.
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
pub enum ErrorKind {
    /// The transport could not complete a network operation.
    Network,
    /// A transport-owned deadline expired.
    Timeout,
    /// Redirect policy refused a destination, hidden hop or exhausted hop limit.
    RedirectPolicy,
    /// The native SDK refused a response whose Content-Encoding the host left
    /// undecoded. Native body-read errors cannot establish a codec failure and
    /// are Network; identified transport deadlines instead yield Timeout.
    /// Portable transports may also report their own decoding failures here.
    Decode,
    /// The transport reports cancellation or abortion.
    Aborted,
    /// The transport cannot classify the failure more specifically.
    Other,
}
#[derive(Clone, Debug, Serialize)]
/// Caller-transport failure during request or body reading. Its message is supplied by the adapter, not normative evidence.
pub struct RequestError {
    /// Transport failure category.
    pub kind: ErrorKind,
    /// Adapter-provided explanation; adapters should remove credentials, URLs and source data from default diagnostics.
    pub message: String,
}
impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for RequestError {}

/// A decoded response body. `read` must not write outside `output` and returns
/// zero only at EOF. Dropping the body must release/cancel host response state.
/// Futures need not be Send, permitting local browser/Worker transports.
pub trait ResponseBody {
    /// Read at most `output.len()` decoded bytes and return their count; zero means EOF. Do not return more than the buffer length. Dropping the body must release/cancel acquisition; futures need not be `Send`.
    fn read(&mut self, output: &mut [u8]) -> impl Future<Output = Result<usize, RequestError>>;
    /// Optional bounded cleanup for connection reuse. The default closes on drop.
    /// Implementations must bound their own cleanup time and bytes; failures must
    /// never replace an already observed HTTP status.
    fn discard_status(&mut self) -> impl Future<Output = ()> {
        std::future::ready(())
    }
}
/// One response before bounded body reading. Portable transports expose individual hops;
/// credentials, TLS, DNS/peer restrictions and decoding remain host obligations.
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

/// Whole-attempt deadline policy. Zero durations are configuration errors.
#[derive(Clone, Copy, Debug, Default)]
pub enum AttemptTimeout {
    /// SDK default: thirty seconds.
    #[default]
    Default,
    /// A positive finite duration, including synchronous validation boundaries.
    After(std::time::Duration),
    /// Explicitly omit the attempt deadline. Caller cancellation still applies.
    Unbounded,
}
impl AttemptTimeout {
    /// Resolve the default or override; reject zero rather than disabling the timer.
    pub fn duration(self) -> Result<Option<std::time::Duration>, ConfigurationError> {
        match self {
            Self::Default => Ok(Some(std::time::Duration::from_secs(
                DEFAULT_TIMEOUT_SECONDS,
            ))),
            Self::After(duration) if !duration.is_zero() => Ok(Some(duration)),
            Self::Unbounded => Ok(None),
            Self::After(_) => Err(ConfigurationError {
                code: "invalid-timeout",
                message: "attempt timeout must be positive, or explicitly unbounded",
            }),
        }
    }
}

/// Host-owned monotonic clock and wake-capable timer for portable discovery.
/// `now` and `sleep_until` must use the same epoch. Polls must be cooperative;
/// dropping the timer future must release it. Local futures are supported.
pub trait DiscoveryTimer {
    /// Monotonic time elapsed since the host's clock epoch.
    fn now(&self) -> std::time::Duration;
    /// Wake the attempt when the absolute monotonic deadline is reached.
    fn sleep_until(&self, deadline: std::time::Duration) -> impl Future<Output = ()>;
}

#[derive(Clone, Copy, Debug)]
/// Portable discovery configuration. Defaults to 1 MiB and thirty seconds.
pub struct ClientOptions {
    /// Positive bound on decoded bytes, with room for one over-limit sentinel.
    pub max_document_bytes: usize,
    /// Whole-attempt deadline, never reset by redirects or successful reads.
    pub timeout: AttemptTimeout,
}
impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            max_document_bytes: DEFAULT_MAX_DOCUMENT_BYTES,
            timeout: AttemptTimeout::default(),
        }
    }
}

/// Retrieve individual hops with a host-owned timer. The callback MUST NOT
/// follow redirects, must dispatch the supplied normalized URL unchanged,
/// enforce application DNS/peer/proxy restrictions at connection time, deliver
/// decoded bytes (or a typed decode refusal), and cancel/release work on drop.
/// The SDK follows at most ten admitted DISC-C-02 redirects. It stores no cache.
pub async fn discover<B, F, T, C>(
    origin: &str,
    options: &ClientOptions,
    control: &CancellationToken,
    timer: &C,
    transport: T,
) -> Result<DiscoveryResult, ConfigurationError>
where
    B: ResponseBody,
    F: Future<Output = Result<HttpResponse<B>, RequestError>>,
    T: FnMut(DiscoveryRequest) -> F,
    C: DiscoveryTimer,
{
    options
        .discover_using(origin, control, timer, transport, true)
        .await
}

impl ClientOptions {
    /// Resolve the positive decoded-byte limit with room for one over-limit sentinel.
    pub fn byte_limit(&self) -> Result<usize, ConfigurationError> {
        match self.max_document_bytes {
            0 | usize::MAX => Err(ConfigurationError {
                code: "invalid-byte-limit",
                message: "maximum document bytes must be positive and fit the host's address space with one sentinel byte",
            }),
            limit => Ok(limit),
        }
    }

    pub(crate) async fn discover_using<B, F, T, C>(
        &self,
        origin: &str,
        control: &CancellationToken,
        timer: &C,
        mut transport: T,
        individual_hops: bool,
    ) -> Result<DiscoveryResult, ConfigurationError>
    where
        B: ResponseBody,
        F: Future<Output = Result<HttpResponse<B>, RequestError>>,
        T: FnMut(DiscoveryRequest) -> F,
        C: DiscoveryTimer,
    {
        let limit = self.byte_limit()?;
        let duration = self.timeout.duration()?;
        let requested_url = endpoint(origin)?;
        let deadline = duration
            .map(|duration| {
                timer.now().checked_add(duration).ok_or(ConfigurationError {
                    code: "invalid-timeout",
                    message: "attempt deadline exceeds the host clock range",
                })
            })
            .transpose()?;
        // Exactly one timer per attempt, owned by the host facility and dropped on every exit.
        let expiry = async {
            if let Some(deadline) = deadline {
                timer.sleep_until(deadline).await
            } else {
                std::future::pending::<()>().await
            }
        };
        let mut expiry = pin!(expiry);
        let mut result = DiscoveryResult {
            metadata: None,
            body: None,
            outcome: DiscoveryOutcome::Cancelled,
        };
        let mut current = url::Url::parse(&requested_url).expect("admitted endpoint");
        let mut followed = 0;
        let mut response = loop {
            // Check before calling the callback, which itself may start I/O synchronously.
            if let Some(outcome) = stopped(control, timer, deadline) {
                result.outcome = outcome;
                return Ok(result);
            }
            let acquired = until_stopped(
                control,
                timer,
                deadline,
                expiry.as_mut(),
                transport(DiscoveryRequest {
                    url: current.to_string(),
                    accept: ACCEPT,
                }),
            )
            .await;
            let mut response = match acquired {
                Err(outcome) => {
                    result.outcome = outcome;
                    return Ok(result);
                }
                Ok(Err(failure)) => {
                    result.outcome = request_failure(failure, false);
                    return Ok(result);
                }
                Ok(Ok(response)) => response,
            };
            result.metadata = Some(ResponseMetadata {
                requested_url: requested_url.clone(),
                final_url: response.final_url.take(),
                status: response.status,
                headers: std::mem::take(&mut response.headers),
            });
            let metadata = result.metadata.as_ref().unwrap();
            if let Some(final_url) = &metadata.final_url {
                let observed = url::Url::parse(final_url);
                let admitted = observed.as_ref().ok().is_some_and(|url| {
                    admit_redirect(&current, url).is_ok() && (!individual_hops || url == &current)
                });
                if !admitted {
                    result.outcome = policy_failure(
                        "transport response URL violates the hop contract or destination policy",
                    );
                    return Ok(result);
                }
            }
            // F2: a host-following transport must not return a redirect with Location.
            // tower-http can do this for malformed Location without consulting reqwest's policy.
            if !individual_hops
                && matches!(response.status, 301 | 302 | 303 | 307 | 308)
                && metadata.header("location").is_some()
            {
                result.outcome =
                    policy_failure("host did not follow the observed redirect Location");
                return Ok(result);
            }
            if individual_hops
                && matches!(response.status, 301 | 302 | 303 | 307 | 308)
                && let Some(location) = metadata.header("location")
            {
                let next = std::str::from_utf8(location)
                    .ok()
                    .filter(|location| {
                        !location.contains('\\') && !location.bytes().any(|b| b <= b' ' || b == 127)
                    })
                    .filter(|location| fluent_uri::UriRef::parse(*location).is_ok())
                    .and_then(|location| current.join(location).ok());
                let next = match next {
                    Some(mut next)
                        if admit_redirect(&current, &next).is_ok() && followed < MAX_REDIRECTS =>
                    {
                        next.set_fragment(None);
                        next
                    }
                    _ => {
                        result.outcome =
                            policy_failure("redirect destination or redirect limit refused");
                        return Ok(result);
                    }
                };
                // Drop the redirect body before dispatch; no redirect body is an OBI.
                drop(response);
                current = next;
                followed += 1;
                continue;
            }
            break response;
        };
        if response.status != 200 {
            result.outcome = match response.status {
                404 => DiscoveryOutcome::Absent,
                401 | 403 => DiscoveryOutcome::Gated,
                _ => DiscoveryOutcome::HttpStatus,
            };
            // Status is committed. Cleanup has the remaining attempt time, never a fresh allowance.
            let _ = until_stopped(
                control,
                timer,
                deadline,
                expiry.as_mut(),
                response.body.discard_status(),
            )
            .await;
            return Ok(result);
        }
        let mut body = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let remaining = (limit + 1 - body.len()).min(buffer.len());
            match until_stopped(control, timer, deadline, expiry.as_mut(), async {
                let read = response.body.read(&mut buffer[..remaining]).await;
                // F3: successful EOF proves this bounded body complete before until_stopped's
                // post-poll check. Preserve it even when that check observes expiry/cancellation.
                if matches!(read, Ok(0)) {
                    result.body = Some(Arc::from(std::mem::take(&mut body)));
                }
                read
            })
            .await
            {
                Err(outcome) => {
                    result.outcome = outcome;
                    return Ok(result);
                }
                Ok(Err(failure)) => {
                    result.outcome = request_failure(failure, true);
                    return Ok(result);
                }
                Ok(Ok(0)) => break,
                Ok(Ok(n)) if n <= remaining => body.extend_from_slice(&buffer[..n]),
                Ok(Ok(_)) => {
                    result.outcome = DiscoveryOutcome::BodyError {
                        error: RequestError {
                            kind: ErrorKind::Other,
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
        if let Some(outcome) = stopped(control, timer, deadline) {
            result.outcome = outcome;
            return Ok(result);
        }
        result.outcome = validate_body(result.body.as_deref().unwrap());
        if let Some(outcome) = stopped(control, timer, deadline) {
            result.outcome = outcome;
        }
        Ok(result)
    }
}
fn request_failure(error: RequestError, body: bool) -> DiscoveryOutcome {
    if error.kind == ErrorKind::Timeout {
        DiscoveryOutcome::Timeout
    } else if body {
        DiscoveryOutcome::BodyError { error }
    } else {
        DiscoveryOutcome::TransportError { error }
    }
}
fn policy_failure(message: &str) -> DiscoveryOutcome {
    DiscoveryOutcome::TransportError {
        error: RequestError {
            kind: ErrorKind::RedirectPolicy,
            message: message.into(),
        },
    }
}
fn stopped<C: DiscoveryTimer>(
    control: &CancellationToken,
    timer: &C,
    deadline: Option<std::time::Duration>,
) -> Option<DiscoveryOutcome> {
    // HD-2: deterministic tie rule, caller cancellation wins when both are ready.
    if control.is_cancelled() {
        Some(DiscoveryOutcome::Cancelled)
    } else if deadline.is_some_and(|deadline| timer.now() >= deadline) {
        Some(DiscoveryOutcome::Timeout)
    } else {
        None
    }
}
async fn until_stopped<C: DiscoveryTimer, F: Future, E: Future<Output = ()>>(
    control: &CancellationToken,
    timer: &C,
    deadline: Option<std::time::Duration>,
    mut expiry: std::pin::Pin<&mut E>,
    future: F,
) -> Result<F::Output, DiscoveryOutcome> {
    let mut future = pin!(future);
    let mut cancelled = pin!(control.cancelled());
    poll_fn(|cx| {
        if cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(DiscoveryOutcome::Cancelled));
        }
        if let Some(outcome) = stopped(control, timer, deadline) {
            return Poll::Ready(Err(outcome));
        }
        if expiry.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(DiscoveryOutcome::Timeout));
        }
        match future.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(value) => Poll::Ready(match stopped(control, timer, deadline) {
                Some(outcome) => Err(outcome),
                None => Ok(value),
            }),
        }
    })
    .await
}
#[derive(Clone, Debug)]
/// Acquisition and validation partition. Only `Found` carries normative proof; HTTP status, refusal and incomplete work remain distinct.
pub enum DiscoveryOutcome {
    /// A complete bounded 200 body established normative document conformance.
    Found {
        #[doc = "Retained immutable proof for the exact acquired body."]
        document: ValidatedDocument,
    },
    /// A complete 200 body established at least one normative violation, including invalid JSON.
    NonConformant {
        #[doc = "Report and parsed snapshot when input was admitted."]
        validation: DocumentValidation,
    },
    /// A complete 200 body could not be proved conformant or nonconformant within supported limits/capabilities.
    Undetermined {
        #[doc = "Report preserving inconclusive evidence and any admitted snapshot."]
        validation: DocumentValidation,
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
    /// Caller cancellation was observed during acquisition or at validation boundaries.
    Cancelled,
    /// The whole-attempt or shorter transport deadline expired. Distinct from caller cancellation.
    Timeout,
    /// The request failed before response metadata was available.
    TransportError {
        #[doc = "Transport classification and adapter-owned explanation."]
        error: RequestError,
    },
    /// The response was obtained but decoded-body reading failed; response metadata remains available.
    BodyError {
        #[doc = "Body-reader classification and adapter-owned explanation."]
        error: RequestError,
    },
}
#[derive(Clone)]
/// Finite discovery receipt: semantic outcome, observed response metadata and complete bounded body when available. No response cache or server is created.
pub struct DiscoveryResult {
    /// Distinct acquisition or validation outcome; do not collapse absence, gating and inability into invalidity.
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
pub fn validate_body(body: &[u8]) -> DiscoveryOutcome {
    match validate_document(body) {
        Err(refusal) => DiscoveryOutcome::VersionRefused { refusal },
        Ok(validation) => match validation.report().conclusion {
            Conformance::Conformant => DiscoveryOutcome::Found {
                document: validation
                    .validated()
                    .expect("conformant validation has a document"),
            },
            Conformance::NonConformant => DiscoveryOutcome::NonConformant { validation },
            Conformance::Undetermined => DiscoveryOutcome::Undetermined { validation },
        },
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod review_tests {
    use super::*;
    struct Clock;
    impl DiscoveryTimer for Clock {
        fn now(&self) -> std::time::Duration {
            std::time::Duration::ZERO
        }
        async fn sleep_until(&self, _: std::time::Duration) {
            std::future::pending::<()>().await;
        }
    }
    struct Empty;
    impl ResponseBody for Empty {
        async fn read(&mut self, _: &mut [u8]) -> Result<usize, RequestError> {
            panic!("redirect body must not be read")
        }
    }
    #[tokio::test(flavor = "current_thread")]
    async fn malformed_location_is_refused_for_exposed_and_host_followed_hops() {
        // S5b review F2 / HD-7: identical visible redirects have identical typed outcomes.
        for individual_hops in [true, false] {
            let r = ClientOptions::default()
                .discover_using(
                    "https://example.test",
                    &CancellationToken::new(),
                    &Clock,
                    |_| async {
                        Ok(HttpResponse {
                            status: 302,
                            final_url: None,
                            headers: vec![Header::new("location", "http://[")],
                            body: Empty,
                        })
                    },
                    individual_hops,
                )
                .await
                .unwrap();
            assert!(matches!(
                r.outcome,
                DiscoveryOutcome::TransportError {
                    error: RequestError {
                        kind: ErrorKind::RedirectPolicy,
                        ..
                    }
                }
            ));
            assert_eq!(r.metadata.unwrap().status, 302);
            assert!(r.body.is_none());
        }
    }
}
