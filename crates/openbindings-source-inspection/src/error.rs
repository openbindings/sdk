use std::fmt;

/// Classification of a refused inspection or invalid provider response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InspectionErrorKind {
    /// The request fails the SDK's carrier admission checks.
    InvalidInput,
    /// The provider does not support this exact source kind for inspection.
    UnsupportedKind,
    /// The provider could not read the source at all; no inventory is established.
    UnreadableSource,
    /// Inspection failed for a provider-specific reason.
    ProviderFailure,
    /// A provider response fails the SDK's structural or result-policy checks.
    InvalidResult,
    /// The result could not be encoded within the selected exact JSON limits.
    Encoding,
    /// The caller cancelled the operation.
    Cancelled,
}

/// A refusal, distinct from a successful but incomplete inventory.
///
/// `code` is a stable provider- or companion-specific identifier; `message` is
/// diagnostic prose. Neither should include credentials or entire source bodies.
#[derive(Clone, PartialEq, Eq)]
pub struct InspectionError {
    kind: InspectionErrorKind,
    code: String,
    message: String,
}
impl std::fmt::Debug for InspectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InspectionError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl InspectionError {
    /// Construct a refusal from a provider. Prefer a namespaced, stable code.
    pub fn new(
        kind: InspectionErrorKind,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            code: code.into(),
            message: message.into(),
        }
    }
    /// Stable category for control flow.
    pub fn kind(&self) -> InspectionErrorKind {
        self.kind
    }
    /// Stable reason code scoped to its provider or companion.
    pub fn code(&self) -> &str {
        &self.code
    }
    /// Human-facing explanation of the refusal.
    pub fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn input(message: impl Into<String>) -> Self {
        Self::new(
            InspectionErrorKind::InvalidInput,
            "openbindings.inspection.invalid-input",
            message,
        )
    }
    pub(crate) fn result(message: impl Into<String>) -> Self {
        Self::new(
            InspectionErrorKind::InvalidResult,
            "openbindings.inspection.invalid-result",
            message,
        )
    }
    pub(crate) fn cancelled() -> Self {
        Self::new(
            InspectionErrorKind::Cancelled,
            "openbindings.inspection.cancelled",
            "source inspection was cancelled",
        )
    }
}

impl fmt::Display for InspectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for InspectionError {}
