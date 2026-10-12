use crate::{Synthesis, SynthesisCandidate};
use openbindings::{AuthoringError, DocumentAssessment, VersionRefusal};
use std::fmt;
/// Stable control-flow classifications; codes provide specific reasons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SynthesisErrorKind {
    /// Invalid request carrier, version or conflicting name.
    InvalidInput,
    /// An exact kind is unsupported for synthesis.
    UnsupportedKind,
    /// Supported kinds but unsupported composition or request shape.
    UnsupportedRequest,
    /// A source is wholly unreadable.
    UnreadableSource,
    /// A provider-specific failure.
    ProviderFailure,
    /// The result fails SDK document, coverage or correspondence policy checks.
    InvalidResult,
    /// Strict completeness refused; the checked result is retained.
    Incomplete,
    /// Exact encoding failed or exceeded its selected limits.
    Encoding,
    /// Caller cancellation.
    Cancelled,
}
/// Located report/request error. Paths use JSON Pointer syntax.
#[derive(Clone, Debug)]
pub struct SynthesisFinding {
    /// Machine-readable reason.
    pub code: String,
    /// Location in the result envelope or request.
    pub path: String,
    /// Explanation, not an identifier.
    pub message: String,
}
/// Rejected provider candidate, never a checked partial success.
#[derive(Clone, Debug)]
pub struct RejectedSynthesis {
    /// Original candidate, available for diagnosis or explicit repair.
    pub candidate: SynthesisCandidate,
    /// Core assessment when the declared version allowed one.
    pub assessment: Option<DocumentAssessment>,
    /// Specific structural/correspondence failure, if any.
    pub finding: Option<SynthesisFinding>,
}
#[derive(Clone, Debug)]
enum Cause {
    Authoring(AuthoringError),
    Version(VersionRefusal),
    Provider(Box<SynthesisError>),
}
/// Typed refusal retaining recovery evidence without another provider call.
#[derive(Clone, Debug)]
pub struct SynthesisError {
    kind: SynthesisErrorKind,
    code: String,
    message: String,
    partial: Option<Box<Synthesis>>,
    rejected: Option<Box<RejectedSynthesis>>,
    cause: Option<Box<Cause>>,
}
impl SynthesisError {
    /// Construct a provider refusal. Reserved facade categories remain subject to facade checks.
    pub fn new(
        kind: SynthesisErrorKind,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            code: code.into(),
            message: message.into(),
            partial: None,
            rejected: None,
            cause: None,
        }
    }
    /// Classification for control flow.
    pub fn kind(&self) -> SynthesisErrorKind {
        self.kind
    }
    /// Stable specific reason.
    pub fn code(&self) -> &str {
        &self.code
    }
    /// Human-readable explanation.
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Checked result retained by strict incompleteness.
    pub fn partial(&self) -> Option<&Synthesis> {
        self.partial.as_deref()
    }
    /// Recover a checked partial result without reacquisition.
    pub fn into_partial(self) -> Option<Synthesis> {
        self.partial.map(|v| *v)
    }
    /// Rejected provider output and diagnosis, distinct from partial success.
    pub fn rejected(&self) -> Option<&RejectedSynthesis> {
        self.rejected.as_deref()
    }
    /// Recover invalid provider evidence for explicit repair.
    pub fn into_rejected(self) -> Option<RejectedSynthesis> {
        self.rejected.map(|v| *v)
    }
    pub(crate) fn simple(kind: SynthesisErrorKind, code: &str, message: impl Into<String>) -> Self {
        Self::new(kind, format!("openbindings.synthesis.{code}"), message)
    }
    pub(crate) fn checked_provider_error(self) -> Self {
        if self.kind != SynthesisErrorKind::Incomplete {
            return self;
        }
        let mut e = Self::simple(
            SynthesisErrorKind::ProviderFailure,
            "reserved-provider-error",
            "provider returned reserved strict-incompleteness category; return a candidate for facade validation",
        );
        e.cause = Some(Box::new(Cause::Provider(Box::new(self))));
        e
    }
    pub(crate) fn unsupported_version(declared: &str) -> Self {
        let refusal = VersionRefusal {
            declared: declared.into(),
            supported: openbindings::SUPPORTED_VERSIONS,
        };
        let mut e = Self::simple(
            SynthesisErrorKind::InvalidInput,
            "unsupported-version",
            refusal.to_string(),
        );
        e.cause = Some(Box::new(Cause::Version(refusal)));
        e
    }
    pub(crate) fn cancelled() -> Self {
        Self::simple(
            SynthesisErrorKind::Cancelled,
            "cancelled",
            "synthesis cancelled",
        )
    }
    pub(crate) fn incomplete(s: Synthesis) -> Self {
        let mut e = Self::simple(
            SynthesisErrorKind::Incomplete,
            "incomplete",
            "strict synthesis requires complete admitted coverage",
        );
        e.partial = Some(Box::new(s));
        e
    }
    pub(crate) fn invalid(
        candidate: SynthesisCandidate,
        assessment: Option<DocumentAssessment>,
        finding: Option<SynthesisFinding>,
        version: Option<VersionRefusal>,
    ) -> Self {
        let mut e = Self::simple(
            SynthesisErrorKind::InvalidResult,
            "invalid-result",
            finding.as_ref().map_or(
                "provider document did not establish core conformance",
                |f| f.message.as_str(),
            ),
        );
        e.rejected = Some(Box::new(RejectedSynthesis {
            candidate,
            assessment,
            finding,
        }));
        e.cause = version.map(|v| Box::new(Cause::Version(v)));
        e
    }
}
impl From<AuthoringError> for SynthesisError {
    fn from(cause: AuthoringError) -> Self {
        let mut e = Self::simple(
            SynthesisErrorKind::ProviderFailure,
            "authoring",
            cause.to_string(),
        );
        e.cause = Some(Box::new(Cause::Authoring(cause)));
        e
    }
}
impl fmt::Display for SynthesisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for SynthesisError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.cause.as_deref() {
            Some(Cause::Authoring(e)) => Some(e),
            Some(Cause::Version(e)) => Some(e),
            Some(Cause::Provider(e)) => Some(e.as_ref()),
            None => None,
        }
    }
}
