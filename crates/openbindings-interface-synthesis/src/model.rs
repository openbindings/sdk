use crate::SynthesisError;
use openbindings::{Conformant, Document, JsonLimits, JsonValue, Source};
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::collections::BTreeMap;

/// Borrowed source plus an optional author-owned emitted key.
#[derive(Clone, Copy)]
pub struct SynthesisSource<'a> {
    /// Exact emitted source name, if specified.
    pub name: Option<&'a str>,
    /// Unchanged source value to interpret and emit.
    pub source: &'a Source,
}
impl std::fmt::Debug for SynthesisSource<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SynthesisSource")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

/// Borrowed synthesis request. Default requests a source-free skeleton.
#[derive(Clone, Copy, Default)]
pub struct SynthesisRequest<'a> {
    /// All supplied inputs; a provider must compose all or refuse.
    pub sources: &'a [SynthesisSource<'a>],
    /// Target declaration; omission uses the core authoring version.
    pub openbindings_version: Option<&'a str>,
    /// Interface name override.
    pub name: Option<&'a str>,
    /// Interface contract version override.
    pub version: Option<&'a str>,
    /// Interface description override.
    pub description: Option<&'a str>,
}
impl std::fmt::Debug for SynthesisRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SynthesisRequest")
            .field("sources", &self.sources)
            .finish_non_exhaustive()
    }
}

/// What the source unit represents, independently of its disposition.
///
/// This closed partition may be exhaustively matched:
/// ```
/// use openbindings_interface_synthesis::CoverageScope;
/// fn label(scope: CoverageScope) -> &'static str {
///     match scope {
///         CoverageScope::Target => "target",
///         CoverageScope::Alternative => "alternative",
///         CoverageScope::Projection => "projection",
///         CoverageScope::Dependency => "dependency",
///     }
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoverageScope {
    /// Addressable target; shares identity with this provider's inspection.
    Target,
    /// Independently selectable invocation alternative.
    Alternative,
    /// Fidelity concern associated with a represented path.
    Projection,
    /// Declared consumption point instead of a binding.
    Dependency,
}
/// Disposition of an observed source unit.
///
/// This closed partition may be exhaustively matched:
/// ```
/// use openbindings_interface_synthesis::CoverageStatus;
/// fn label(status: CoverageStatus) -> &'static str {
///     match status {
///         CoverageStatus::Represented => "represented",
///         CoverageStatus::Excluded => "excluded",
///         CoverageStatus::Invalid => "invalid",
///         CoverageStatus::Lossy => "lossy",
///         CoverageStatus::ImplementationUnsupported => "implementation-unsupported",
///     }
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoverageStatus {
    /// Faithfully represented in the emitted document.
    Represented,
    /// Excluded by a revision-identifiable kind rule.
    Excluded,
    /// Invalid under the kind or an incorporated authority.
    Invalid,
    /// A usable represented path with disclosed projection loss.
    Lossy,
    /// Admitted by the kind but not represented by this implementation.
    ImplementationUnsupported,
}
/// Editable provider evidence. Its correspondence is checked against the request
/// and document before delivery; it is not independently serializable as success.
///
/// Providers start one with [`Self::new`] and assign the evidence and path
/// fields its disposition requires. Later versions may add fields: read them by
/// name and match with `..`.
/// ```compile_fail,E0639
/// use openbindings_interface_synthesis::{CoverageEntry, CoverageScope, CoverageStatus};
/// let _ = CoverageEntry { source_index: 0, source_ref: "echo".into(),
///     scope: CoverageScope::Target, status: CoverageStatus::Excluded,
///     source_key: None, operation_key: None, binding_key: None, dependency_key: None,
///     reason_code: None, rule: None, message: None, requirements: None, details: None,
///     additional_fields: Default::default() };
/// ```
/// ```
/// use openbindings_interface_synthesis::{CoverageEntry, CoverageScope, CoverageStatus};
/// let mut entry = CoverageEntry::new(0, "echo", CoverageScope::Target, CoverageStatus::Excluded);
/// entry.reason_code = Some("example.excluded".into());
/// entry.rule = Some("urn:example:kind:v1".into());
/// entry.message = Some("excluded by the kind".into());
/// let CoverageEntry { source_index, source_ref, scope, status, source_key, operation_key,
///     binding_key, dependency_key, reason_code, rule, message, requirements, details,
///     additional_fields, .. } = entry;
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub struct CoverageEntry {
    /// Index in the original request's source slice.
    pub source_index: usize,
    /// Stable source-local unit identifier.
    pub source_ref: String,
    /// Kind of source unit.
    pub scope: CoverageScope,
    /// What became of the unit.
    pub status: CoverageStatus,
    /// Emitted source key for represented/lossy binding paths.
    pub source_key: Option<String>,
    /// Emitted operation key for represented/lossy binding paths.
    pub operation_key: Option<String>,
    /// Emitted binding key for represented/lossy binding paths.
    pub binding_key: Option<String>,
    /// Emitted dependency key, only for represented/lossy dependencies.
    pub dependency_key: Option<String>,
    /// Stable reason; required unless represented.
    pub reason_code: Option<String>,
    /// Revision-identifiable kind rule; required for exclusions.
    pub rule: Option<String>,
    /// Explanation; required unless represented.
    pub message: Option<String>,
    /// Named runtime prerequisites, not credentials or retry policy.
    pub requirements: Option<Vec<String>>,
    /// Exact structured evidence.
    pub details: Option<BTreeMap<String, JsonValue>>,
    /// Additional evidence permitted by this open object. Cannot shadow fields.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl std::fmt::Debug for CoverageEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoverageEntry")
            .field("source_index", &self.source_index)
            .field("scope", &self.scope)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl CoverageEntry {
    /// Start a unit disposition; populate its required evidence/path fields before returning it.
    pub fn new(
        source_index: usize,
        source_ref: impl Into<String>,
        scope: CoverageScope,
        status: CoverageStatus,
    ) -> Self {
        Self {
            source_index,
            source_ref: source_ref.into(),
            scope,
            status,
            source_key: None,
            operation_key: None,
            binding_key: None,
            dependency_key: None,
            reason_code: None,
            rule: None,
            message: None,
            requirements: None,
            details: None,
            additional_fields: BTreeMap::new(),
        }
    }
}
/// Evidence explaining an unknown remainder, distinct from known unsupported units.
///
/// Providers build one with [`Self::new`], [`Self::with_details`] and
/// [`Self::with_additional_fields`]. Later versions may add fields: read them by
/// name and match with `..`.
#[derive(Clone)]
#[non_exhaustive]
pub struct CoverageLimitation {
    /// Nonempty stable namespaced code.
    pub code: String,
    /// Nonempty explanation.
    pub message: String,
    /// Optional exact evidence.
    pub details: Option<BTreeMap<String, JsonValue>>,
    /// Additional evidence; cannot shadow known fields.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl CoverageLimitation {
    /// Evidence with a stable namespaced code and an explanation, both nonempty.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
            additional_fields: BTreeMap::new(),
        }
    }
    /// Attach exact structured evidence.
    #[must_use]
    pub fn with_details(mut self, details: BTreeMap<String, JsonValue>) -> Self {
        self.details = Some(details);
        self
    }
    /// Attach additional evidence members; they must not shadow known fields.
    #[must_use]
    pub fn with_additional_fields(mut self, fields: BTreeMap<String, JsonValue>) -> Self {
        self.additional_fields = fields;
        self
    }
}
impl std::fmt::Debug for CoverageLimitation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoverageLimitation").finish_non_exhaustive()
    }
}

/// Provider-authored report before request/document correspondence checks.
///
/// Providers build one with [`Self::complete`] or [`Self::partial`]; the public
/// fields stay assignable. Later versions may add fields: read them by name and
/// match with `..`.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct CoverageClaim {
    /// One disposition per observed unit identity.
    pub entries: Vec<CoverageEntry>,
    /// Whether the complete inventory has been established.
    pub exhaustive: bool,
    /// Required when the inventory is not exhaustive.
    pub limitation: Option<CoverageLimitation>,
    /// Additional report evidence; cannot shadow known fields.
    pub additional_fields: BTreeMap<String, JsonValue>,
}
impl CoverageClaim {
    /// Claim an exhaustive inventory of the supplied dispositions, with no limitation.
    ///
    /// The facade still checks this claim against the request and candidate document.
    pub fn complete(entries: Vec<CoverageEntry>) -> Self {
        Self {
            entries,
            exhaustive: true,
            ..Self::default()
        }
    }
    /// Claim an incomplete inventory of the supplied dispositions, with evidence
    /// for the unknown remainder.
    ///
    /// The facade still checks this claim against the request and candidate document.
    pub fn partial(entries: Vec<CoverageEntry>, limitation: CoverageLimitation) -> Self {
        Self {
            entries,
            exhaustive: false,
            limitation: Some(limitation),
            ..Self::default()
        }
    }
}
impl std::fmt::Debug for CoverageClaim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoverageClaim")
            .field("entries", &self.entries)
            .field("exhaustive", &self.exhaustive)
            .field("limitation", &self.limitation)
            .finish_non_exhaustive()
    }
}

/// Provider output; neither core conformance nor coverage correspondence is implied.
///
/// Providers build one with [`Self::new`]. Later versions may add fields: read
/// them by name and match with `..`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct SynthesisCandidate {
    /// Editable exact candidate document to validate.
    pub document: Document,
    /// Editable coverage claim.
    pub coverage: CoverageClaim,
}
impl SynthesisCandidate {
    /// A candidate document with its coverage claim.
    pub fn new(document: Document, coverage: CoverageClaim) -> Self {
        Self { document, coverage }
    }
}
/// Checked immutable report. Its flags express evidence, not proof of provider honesty.
#[derive(Clone)]
pub struct SynthesisCoverage {
    pub(crate) claim: CoverageClaim,
    pub(crate) source_keys: Vec<String>,
}
impl std::fmt::Debug for SynthesisCoverage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SynthesisCoverage")
            .field("claim", &self.claim)
            .finish_non_exhaustive()
    }
}

impl SynthesisCoverage {
    /// Emitted source key for every input, in original request order.
    /// Equal inputs may share a key. Unnamed inputs with multiple equal emitted
    /// sources use the lexicographically first key; binding paths may use another
    /// equal source. Supplied names are always preserved.
    pub fn source_keys(&self) -> &[String] {
        &self.source_keys
    }
    /// Borrow all dispositions in provider order.
    pub fn entries(&self) -> &[CoverageEntry] {
        &self.claim.entries
    }
    /// Whether the provider established its complete inventory.
    pub fn exhaustive(&self) -> bool {
        self.claim.exhaustive
    }
    /// Derived full upstream representation, distinct from strict eligibility.
    pub fn fully_represented(&self) -> bool {
        self.exhaustive()
            && self.entries().iter().all(|e| {
                matches!(
                    e.status,
                    CoverageStatus::Represented | CoverageStatus::Invalid
                )
            })
    }
    /// Complete admitted coverage, allowing sound loss and genuine exclusions.
    pub fn strict_eligible(&self) -> bool {
        self.exhaustive()
            && self
                .entries()
                .iter()
                .all(|e| e.status != CoverageStatus::ImplementationUnsupported)
    }
    /// Evidence for an unknown remainder, if provided.
    pub fn limitation(&self) -> Option<&CoverageLimitation> {
        self.claim.limitation.as_ref()
    }
    /// Additional exact report evidence.
    pub fn additional_fields(&self) -> &BTreeMap<String, JsonValue> {
        &self.claim.additional_fields
    }
}
/// Conformant document and checked coverage derived from the same provider call.
#[derive(Clone, Debug)]
pub struct Synthesis {
    pub(crate) document: Conformant<Document>,
    pub(crate) coverage: SynthesisCoverage,
}
impl Synthesis {
    /// Borrow core conformance evidence. Kind soundness remains a provider claim.
    pub fn document(&self) -> &Conformant<Document> {
        &self.document
    }
    /// Borrow coverage without discarding it.
    pub fn coverage(&self) -> &SynthesisCoverage {
        &self.coverage
    }
    /// Transfer both the document and its evidence explicitly.
    pub fn into_parts(self) -> (Conformant<Document>, SynthesisCoverage) {
        (self.document, self.coverage)
    }
    /// Persist document and coverage as an exact JSON envelope with default limits.
    ///
    /// # Errors
    /// Returns an error if the carrier cannot be serialized or its encoded JSON exceeds the admission limits.
    pub fn to_json(&self) -> Result<JsonValue, SynthesisError> {
        self.to_json_with_limits(JsonLimits::default())
    }
    /// Bound emitted bytes and apply core JSON node/depth admission limits.
    /// Does not bound already retained owners or total process memory.
    ///
    /// # Errors
    /// Returns an error if the carrier cannot be serialized or its encoded JSON exceeds the admission limits.
    pub fn to_json_with_limits(&self, limits: JsonLimits) -> Result<JsonValue, SynthesisError> {
        crate::exact::encode(self, limits)
    }
}
impl Serialize for Synthesis {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut m = serializer.serialize_map(Some(2))?;
        m.serialize_entry("interface", self.document.as_document().value())?;
        m.serialize_entry("coverage", &self.coverage)?;
        m.end()
    }
}
impl Serialize for SynthesisCoverage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Entries<'a>(&'a [CoverageEntry]);
        impl Serialize for Entries<'_> {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeSeq;
                let mut seq = s.serialize_seq(Some(self.0.len()))?;
                for e in self.0 {
                    seq.serialize_element(&EntryWire(e))?;
                }
                seq.end()
            }
        }
        let mut m = serializer.serialize_map(None)?;
        m.serialize_entry("sourceKeys", self.source_keys())?;
        m.serialize_entry("entries", &Entries(self.entries()))?;
        m.serialize_entry("exhaustive", &self.exhaustive())?;
        m.serialize_entry("fullyRepresented", &self.fully_represented())?;
        if let Some(l) = self.limitation() {
            m.serialize_entry("limitation", &LimitationWire(l))?;
        }
        for (k, v) in self.additional_fields() {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}
struct EntryWire<'a>(&'a CoverageEntry);
impl Serialize for EntryWire<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let e = self.0;
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("sourceIndex", &e.source_index)?;
        m.serialize_entry("sourceRef", &e.source_ref)?;
        m.serialize_entry("scope", &e.scope)?;
        m.serialize_entry("status", &e.status)?;
        macro_rules! opt {
            ($key:literal,$field:ident) => {
                if let Some(v) = &e.$field {
                    m.serialize_entry($key, v)?;
                }
            };
        }
        opt!("sourceKey", source_key);
        opt!("operationKey", operation_key);
        opt!("bindingKey", binding_key);
        opt!("dependencyKey", dependency_key);
        opt!("reasonCode", reason_code);
        opt!("rule", rule);
        opt!("message", message);
        opt!("requirements", requirements);
        opt!("details", details);
        for (k, v) in &e.additional_fields {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}
struct LimitationWire<'a>(&'a CoverageLimitation);
impl Serialize for LimitationWire<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let l = self.0;
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("code", &l.code)?;
        m.serialize_entry("message", &l.message)?;
        if let Some(v) = &l.details {
            m.serialize_entry("details", v)?;
        }
        for (k, v) in &l.additional_fields {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}
