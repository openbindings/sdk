//! One exact editable document and retained conformance evidence.
use crate::{
    fixed_schema,
    schema_index::{self, DIALECT, SameDocument, SchemaIndex},
    *,
};
use openbindings_internal_json::backend;
use serde::Serialize;
mod edit;
mod read_views;
pub use edit::{Edit, EditError, PointerRef};
mod conformant_views;
use crate::{contracts::ResourceSetInner, schema_space::SchemaSpace};
pub use conformant_views::*;
use read_views::NamespaceCache;
pub use read_views::{BindingView, DependencyView, ExampleView, SourceView};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    sync::{Arc, Mutex, OnceLock, PoisonError, Weak},
};

/// Stable rule identifiers OBI-01 through OBI-13, in specification order.
pub const DOCUMENT_RULES: [&str; 13] = [
    "OBI-01", "OBI-02", "OBI-03", "OBI-04", "OBI-05", "OBI-06", "OBI-07", "OBI-08", "OBI-09",
    "OBI-10", "OBI-11", "OBI-12", "OBI-13",
];
// The document rules, named by their identifiers and declared in
// `DOCUMENT_RULES` order, so `Rule::Obi04 as usize` indexes OBI-04's evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    Obi01,
    Obi02,
    Obi03,
    Obi04,
    Obi05,
    Obi06,
    Obi07,
    Obi08,
    Obi09,
    Obi10,
    Obi11,
    Obi12,
    Obi13,
}
impl Rule {
    const ALL: [Rule; 13] = [
        Rule::Obi01,
        Rule::Obi02,
        Rule::Obi03,
        Rule::Obi04,
        Rule::Obi05,
        Rule::Obi06,
        Rule::Obi07,
        Rule::Obi08,
        Rule::Obi09,
        Rule::Obi10,
        Rule::Obi11,
        Rule::Obi12,
        Rule::Obi13,
    ];
    fn id(self) -> &'static str {
        DOCUMENT_RULES[self as usize]
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Evidence for one normative document rule. This closed partition may be exhaustively matched; it is separate from explanatory findings.
pub enum Evidence {
    /// The rule was established for this snapshot.
    Met,
    /// A rule violation was established, even if other work is inconclusive.
    Violated,
    /// The implementation could not establish whether the rule was met or violated.
    Inconclusive,
    /// The rule does not apply after an earlier prerequisite fails.
    NotApplicable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Overall normative document conclusion. This is a closed semantic partition, not a schema-instance validation result.
pub enum Conformance {
    /// Every applicable document rule was established; a conformant document can be obtained.
    Conformant,
    /// At least one document rule was established as violated.
    NonConformant,
    /// No violation was established, but evidence is insufficient to prove conformance.
    Undetermined,
}
#[derive(Clone, PartialEq, Eq, Serialize)]
/// One explanatory normative-rule finding. Findings may be bounded; the report's independent rule evidence determines its conclusion. Serializes as JSON text; a pointer through a member name with an unpaired UTF-16 unit has no `serde_json::Value` form, so `serde_json::to_value` refuses such a record and `json!` panics: use `serde_json::to_string` or `to_writer`.
///
/// Only validation builds findings. Later versions may add fields: read them by
/// name and match with `..`.
/// ```compile_fail,E0639
/// use openbindings::*;
/// let _ = Finding { rule: "OBI-01", status: Evidence::Violated, code: "value",
///     location: None, message: String::new() };
/// ```
/// ```compile_fail,E0638
/// use openbindings::*;
/// fn read(finding: &Finding) {
///     let Finding { rule, status, code, location, message } = finding;
/// }
/// ```
/// ```
/// use openbindings::*;
/// fn read(finding: &Finding) {
///     let Finding { rule, status, code, location, message, .. } = finding;
/// }
/// ```
#[non_exhaustive]
pub struct Finding {
    /// Normative rule identifier, such as `OBI-01`.
    pub rule: &'static str,
    /// Evidence expressed by this finding, not necessarily the final aggregate for its rule.
    pub status: Evidence,
    /// Stable machine-readable diagnostic identifier; prefer it over message matching.
    pub code: &'static str,
    /// Original-source coordinates, absent only for a finding about no particular position, such as an input limit. Pointers are exact, are data and must be escaped for display; byte columns are not UTF-16 editor columns.
    pub location: Option<SourceLocation>,
    /// Human-facing explanation; display as text and use `rule`, `code` and `status` for logic.
    pub message: String,
}
impl std::fmt::Debug for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rule = if DOCUMENT_RULES.contains(&self.rule) {
            self.rule
        } else {
            "<redacted>"
        };
        // Fixed Checks emissions and the non-limit parser codes forwarded to OBI-01.
        let code = match self.code {
            "bom"
            | "colon"
            | "comma-or-close"
            | "escape"
            | "exponent"
            | "fraction"
            | "literal"
            | "number"
            | "object-key"
            | "string-control"
            | "trailing-token"
            | "unicode-escape"
            | "unterminated-string"
            | "utf8"
            | "value"
            | "input-limit"
            | "schema-mismatch"
            | "fixed-schema-limit-or-failure"
            | "duplicate-member"
            | "unpaired-surrogate"
            | "semver"
            | "missing-version"
            | "name-grammar"
            | "duplicate-operation-name"
            | "unknown-source"
            | "unknown-operation"
            | "schema-dialect"
            | "absolute-schema-id"
            | "reference-type"
            | "reference-form"
            | "reference-target"
            | "duplicate-anchor"
            | "duplicate-schema-id" => self.code,
            _ => "<redacted>",
        };
        f.debug_struct("Finding")
            .field("rule", &rule)
            .field("code", &code)
            .field("status", &self.status)
            .field("location", &self.location)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize)]
/// Assessment of all normative document rules at the pinned specification revision. At most 4096 findings and 8 MiB of aggregate generated-pointer bytes (UTF-8, or two per UTF-16 unit for a pointer through a name with an unpaired surrogate) are retained. Omitted findings set `findings_truncated`; rule evidence and the conclusion remain independent of presentation caps. Retained coordinates always refer to original source. Serializes as JSON text; a pointer through a member name with an unpaired UTF-16 unit has no `serde_json::Value` form, so `serde_json::to_value` refuses such a record and `json!` panics: use `serde_json::to_string` or `to_writer`.
///
/// Only validation builds reports. Later versions may add fields: read them by
/// name and match with `..`.
#[non_exhaustive]
pub struct ValidationReport {
    /// Applied specification release, independent of package version.
    pub release: &'static str,
    /// Exact applied specification Git revision.
    pub revision: &'static str,
    /// Aggregate normative conclusion derived from all rule evidence.
    pub conclusion: Conformance,
    /// One entry for every [`DOCUMENT_RULES`] identifier, including inconclusive and not-applicable rules.
    pub evidence: BTreeMap<&'static str, Evidence>,
    /// Retained explanatory findings; an empty or truncated list alone does not prove conformance.
    pub findings: Vec<Finding>,
    /// True when findings exceed the 4096-entry or 8 MiB aggregate generated-pointer byte cap. Omission never changes rule evidence or fabricates a shortened source pointer.
    pub findings_truncated: bool,
}
impl std::fmt::Debug for ValidationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidationReport")
            .field("conclusion", &self.conclusion)
            .field("findings", &self.findings)
            .field("findings_truncated", &self.findings_truncated)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize)]
/// A well-formed declared version outside this implementation's supported line. This is separate from conformance evidence.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct VersionRefusal {
    /// Original declared version string.
    pub declared: String,
    /// Supported specification line used for the refusal.
    pub supported: &'static str,
}
impl VersionRefusal {
    /// The refusal of a declared version: `Some` for a well-formed version
    /// outside the supported line, `None` when [`check_version`] finds it
    /// supported or malformed. A tool that checks a declaration before any
    /// document exists, such as a synthesis target version, refuses with the
    /// record validation returns.
    pub fn new(declared: &str) -> Option<Self> {
        (check_version(declared) == VersionDecision::Unsupported).then(|| Self {
            declared: declared.into(),
            supported: SUPPORTED_VERSIONS,
        })
    }
}
impl std::fmt::Debug for VersionRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VersionRefusal").finish_non_exhaustive()
    }
}

impl fmt::Display for VersionRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "declared version {} is outside {}",
            self.declared, self.supported
        )
    }
}
impl std::error::Error for VersionRefusal {}
#[derive(Clone)]
/// Exact OBI document, edited in place without a conformance claim. Clones share storage and lazily cached interpretation/validation. Dropping all owners releases storage; it does not promise an immediate process RSS decrease.
pub struct Document {
    pub(crate) inner: Arc<DocumentInner>,
}
pub(crate) struct DocumentInner {
    pub value: JsonValue,
    // The limits this revision was admitted under; edits parse the edited
    // document under them.
    limits: JsonLimits,
    pub schemas: OnceLock<Arc<SchemaIndex>>,
    // Value-contract resolution spaces are assembled only when an evaluator asks
    // for the core projection. The space without supplied resources lives here;
    // a space with a nonempty set lives in that set, which this revision tells
    // to release it when the revision is dropped.
    resolution: OnceLock<Arc<SchemaSpace>>,
    spaces_in: Mutex<Vec<Weak<ResourceSetInner>>>,
    validation: OnceLock<Result<Arc<ValidationReport>, VersionRefusal>>,
    interpretation: OnceLock<Result<(), InterpretationError>>,
    names: OnceLock<Result<NameIndex, InterpretationError>>,
    bindings: NamespaceCache,
    sources: NamespaceCache,
    dependencies: NamespaceCache,
}
impl DocumentInner {
    /// Stop tracking a set that is being dropped.
    pub(crate) fn forget_set(&self, set: *const ResourceSetInner) {
        let mut sets = self
            .spaces_in
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        sets.retain(|known| !std::ptr::eq(known.as_ptr(), set));
        if sets.is_empty() {
            *sets = Vec::new();
        }
    }
}
impl Drop for DocumentInner {
    fn drop(&mut self) {
        let revision: *const Self = self;
        let sets = self
            .spaces_in
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner);
        for set in sets.drain(..) {
            if let Some(set) = set.upgrade() {
                set.release(revision);
            }
        }
    }
}
impl fmt::Debug for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Document")
            .field("value", &self.inner.value)
            .finish()
    }
}
#[derive(Clone, Debug)]
/// Normative evidence plus the document snapshot when one was admitted. Invalid JSON can still produce a report without a document.
/// Inspect the report or obtain a conformant document; unwrapping only checks the version.
///
/// ```compile_fail
/// #![deny(unused_must_use)]
/// use openbindings::Document;
/// Document::parse(r#"{"openbindings":"0.2.0"}"#).unwrap().validate().unwrap();
/// ```
/// ```compile_fail
/// #![deny(unused_must_use)]
/// openbindings::validate_document(r#"{"openbindings":"0.2.0"}"#).unwrap();
/// ```
#[must_use = "inspect the report or obtain a conformant document before continuing"]
pub struct DocumentValidation {
    document: Option<Document>,
    report: Arc<ValidationReport>,
}
#[derive(Clone, Debug)]
/// Retained proof that a document revision met every applicable normative rule.
///
/// Its reads hand out one conformant view per document part,
/// [`ConformantOperationView`], [`ConformantBindingView`], [`ConformantSourceView`],
/// [`ConformantDependencyView`] and [`ConformantExampleView`]: each is the plain
/// view of the same name, such as [`OperationView`], read with this proof, so its
/// field reads are infallible.
///
/// Proof has no public unchecked constructor or deserializer:
/// ```compile_fail
/// use openbindings::*;
/// fn forge(document: Document) { let _ = Conformant { document }; }
/// ```
/// Editing requires giving up this proof:
/// ```compile_fail
/// use openbindings::{Conformant, Document};
/// fn edit(mut proof: Conformant<Document>) { proof.set_description("changed").unwrap(); }
/// ```
/// ```compile_fail
/// use openbindings::{Conformant, Document};
/// fn consume(proof: Conformant<Document>) { let document = proof.into_inner(); proof.operations(); }
/// ```
/// Only a conformant operation hands out value contracts:
/// ```compile_fail,E0599
/// use openbindings::{Document, SchemaEvaluator};
/// fn contracts(doc: Document, evaluator: &dyn SchemaEvaluator) {
///     let operations = doc.operations().unwrap();
///     let _ = operations[0].input().unwrap().prepare(evaluator);
/// }
/// ```
/// Obtain it through [`DocumentValidation::conformant`]; value contract preparation is still a separate evaluator-dependent step.
pub struct Conformant<T> {
    document: T,
}
#[derive(Clone, PartialEq, Eq)]
/// Structured refusal to interpret typed fields. Causes are extensible; match specific cases or `code()` and retain a fallback. Exact parsed JSON remains available after this error.
#[non_exhaustive]
pub enum InterpretationError {
    /// The declaration is unsupported; contains the distinct version refusal.
    Version(VersionRefusal),
    /// The required version declaration is absent, malformed or not a scalar string.
    MalformedVersion,
    /// Duplicate member names make typed interpretation ambiguous.
    DuplicateMembers,
    /// A retained JSON string contains an unpaired UTF-16 unit that typed interpretation cannot represent.
    UnpairedString,
    /// A typed field has an invalid shape, with an original source location.
    InvalidField {
        /// Specific stable identifier for the invalid typed field.
        code: &'static str,
        /// Original coordinate of the offending field/value, not a generated schema location.
        location: SourceLocation,
    },
}
impl std::fmt::Debug for InterpretationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version(value) => f.debug_tuple("Version").field(value).finish(),
            Self::MalformedVersion => f.write_str("MalformedVersion"),
            Self::DuplicateMembers => f.write_str("DuplicateMembers"),
            Self::UnpairedString => f.write_str("UnpairedString"),
            Self::InvalidField { location, .. } => f
                .debug_struct("InvalidField")
                .field("location", location)
                .finish_non_exhaustive(),
        }
    }
}

impl fmt::Display for InterpretationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "document cannot be interpreted: {}", self.code())
    }
}
impl std::error::Error for InterpretationError {}
impl InterpretationError {
    /// Return the specific stable interpretation code; messages are not identifiers.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Version(_) => "unsupported-version",
            Self::MalformedVersion => "malformed-version",
            Self::DuplicateMembers => "duplicate-members",
            Self::UnpairedString => "unpaired-string",
            Self::InvalidField { code, .. } => code,
        }
    }
    /// Borrow a located field error's original coordinates; global interpretation refusals have no invented location.
    pub fn source_location(&self) -> Option<&SourceLocation> {
        match self {
            Self::InvalidField { location, .. } => Some(location),
            _ => None,
        }
    }
    fn invalid(code: &'static str, value: JsonRef<'_>) -> Self {
        Self::InvalidField {
            code,
            location: value.location(),
        }
    }
}

impl Document {
    /// Parse and retain exact bytes and source locations without establishing conformance.
    /// Use [`Self::validate`] before treating this as a conformant document. See the
    /// [Rust first-use guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md)
    /// for the progression from parsing to a prepared input contract.
    ///
    /// # Errors
    /// Returns an input error for invalid UTF-8, a byte order mark, invalid JSON syntax, or an exceeded admission limit.
    pub fn parse(input: impl AsRef<[u8]>) -> Result<Self, InputError> {
        Self::parse_with_limits(input, JsonLimits::default())
    }
    /// Parse under caller-selected byte, node and nesting admission limits, which
    /// the document keeps: its edits and typed setters parse the edited document
    /// under the same limits.
    ///
    /// # Errors
    /// Returns an input error for invalid UTF-8, a byte order mark, invalid JSON syntax, or an exceeded admission limit.
    pub fn parse_with_limits(
        input: impl AsRef<[u8]>,
        limits: JsonLimits,
    ) -> Result<Self, InputError> {
        JsonValue::parse_with_limits(input, limits).map(|value| Self::admitted(value, limits))
    }
    /// Parse as [`Self::parse`] does, taking the bytes: the document keeps them
    /// as its text instead of a copy, so a caller that hands over its buffer
    /// holds one copy of the document, not two.
    ///
    /// # Errors
    /// Returns an input error for invalid UTF-8, a byte order mark, invalid JSON syntax, or an exceeded admission limit.
    pub fn parse_owned(input: Vec<u8>) -> Result<Self, InputError> {
        Self::parse_owned_with_limits(input, JsonLimits::default())
    }
    /// Parse as [`Self::parse_with_limits`] does, taking the bytes as
    /// [`Self::parse_owned`] does.
    ///
    /// # Errors
    /// Returns an input error for invalid UTF-8, a byte order mark, invalid JSON syntax, or an exceeded admission limit.
    pub fn parse_owned_with_limits(input: Vec<u8>, limits: JsonLimits) -> Result<Self, InputError> {
        JsonValue::parse_owned_with_limits(input, limits).map(|value| Self::admitted(value, limits))
    }
    /// Create a document snapshot from an exact value without validation. A subtree is made standalone so subsequent document coordinates refer to its independent source.
    /// The document keeps [`JsonLimits::default`] for its edits;
    /// [`Self::parse_with_limits`] admits one under other limits.
    pub fn from_json(value: JsonValue) -> Self {
        Self::admitted(value, JsonLimits::default())
    }
    pub(crate) fn admitted(value: JsonValue, limits: JsonLimits) -> Self {
        let value = backend::standalone(value);
        Self {
            inner: Arc::new(DocumentInner {
                value,
                limits,
                schemas: OnceLock::new(),
                resolution: OnceLock::new(),
                spaces_in: Mutex::default(),
                validation: OnceLock::new(),
                interpretation: OnceLock::new(),
                names: OnceLock::new(),
                bindings: OnceLock::new(),
                sources: OnceLock::new(),
                dependencies: OnceLock::new(),
            }),
        }
    }
    /// Borrow the exact document value; no new owner or conformance proof is created.
    pub fn value(&self) -> &JsonValue {
        &self.inner.value
    }
    /// The admission limits this document was admitted under, which its edits and
    /// typed setters apply: [`JsonLimits::default`] for [`Self::parse`],
    /// [`Self::new`] and [`Self::from_json`], the caller's for
    /// [`Self::parse_with_limits`] and [`validate_document_with_limits`], and no
    /// nesting cap for [`validate_document`].
    pub fn limits(&self) -> JsonLimits {
        self.inner.limits
    }
    /// Borrow the current revision's exact bytes, including surrounding whitespace.
    pub fn as_bytes(&self) -> &[u8] {
        self.value().original_source()
    }
    /// Assess every document rule, retaining findings at original source locations.
    /// A supported version can still be nonconformant or undetermined; only
    /// [`DocumentValidation::conformant`] yields a conformance proof. An unsupported
    /// declared version is a separate refusal. Explanatory finding messages are
    /// human-readable guidance; use rule/code/evidence fields for program logic.
    ///
    /// # Errors
    /// Returns a version refusal only for a well-formed declaration outside the supported line. Malformed input and other rule violations are returned as validation evidence, not as this error.
    pub fn validate(&self) -> Result<DocumentValidation, VersionRefusal> {
        let report = self
            .inner
            .validation
            .get_or_init(|| assess_value(self).map(Arc::new))
            .clone()?;
        Ok(DocumentValidation {
            document: Some(self.clone()),
            report,
        })
    }
    pub(crate) fn schema_index(&self) -> &SchemaIndex {
        self.shared_schema_index()
    }
    fn shared_schema_index(&self) -> &Arc<SchemaIndex> {
        self.inner
            .schemas
            .get_or_init(|| Arc::new(SchemaIndex::build(self.value())))
    }
    /// The revision's value with its schema index, which preparation shares
    /// instead of indexing the document again. Validation checks each indexed
    /// schema against the meta-schemas through the index (OBI-10), so the
    /// index knows whether they all satisfied them. A revision that has not
    /// built its index, because it was never validated, keeps none: the
    /// document is indexed for this use alone.
    pub(crate) fn indexed(&self) -> openbindings_internal_schema::RawDocument {
        match self.inner.schemas.get() {
            Some(index) => openbindings_internal_schema::RawDocument::shared(index.clone()),
            None => openbindings_internal_schema::RawDocument::new(self.value().clone()),
        }
    }
    pub(crate) fn interpretable(&self) -> Result<(), InterpretationError> {
        self.inner
            .interpretation
            .get_or_init(|| self.check_interpretation())
            .clone()
    }
    fn check_interpretation(&self) -> Result<(), InterpretationError> {
        if let Some(refusal) = version_refusal(self.value()) {
            return Err(InterpretationError::Version(refusal));
        }
        if self.value().has_duplicate_names() {
            return Err(InterpretationError::DuplicateMembers);
        }
        if self
            .value()
            .get("openbindings")
            .and_then(|v| v.as_str())
            .is_none_or(|s| check_version(s) != VersionDecision::Supported)
        {
            return Err(InterpretationError::MalformedVersion);
        }
        Ok(())
    }
    pub(crate) fn names(&self) -> Result<&NameIndex, InterpretationError> {
        self.inner
            .names
            .get_or_init(|| NameIndex::build(self.value()))
            .as_ref()
            .map_err(Clone::clone)
    }
    /// Exact keys and aliases share one namespace. Every repeated occurrence is
    /// unresolved; candidate primary keys are distinct and lexically ordered.
    ///
    /// # Errors
    /// Returns an interpretation error for an unsupported or malformed version, duplicate members, or an invalid shape in the fields read by this method. This does not require document conformance.
    pub fn resolve_operation(&self, name: &str) -> Result<OperationSelection, InterpretationError> {
        self.interpretable()?;
        let index = self.names()?;
        let Some(matches) = index.names.get(name) else {
            return Ok(OperationSelection::Missing);
        };
        if matches.len() > 1 {
            // Deduplicate cheap operation indices before copying public names.
            // Repeated aliases of a long primary key must not copy it per occurrence.
            let mut candidates = matches
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .map(|id| index.primary_keys[id].to_string())
                .collect::<Vec<_>>();
            candidates.sort();
            return Ok(OperationSelection::Ambiguous { candidates });
        }
        let key = &index.primary_keys[matches[0]];
        self.operation_view(key, &index.operations[key])
            .map(OperationSelection::Found)
    }
    fn operation_view(
        &self,
        key: &Arc<str>,
        entry: &OperationEntry,
    ) -> Result<OperationView, InterpretationError> {
        let value = &entry.value;
        if value.kind() != JsonKind::Object {
            return Err(InterpretationError::invalid(
                "invalid-operation-object",
                value.view(),
            ));
        }
        Ok(OperationView {
            document: self.clone(),
            key: key.clone(),
            value: value.clone(),
            examples: entry.examples.clone(),
        })
    }
    /// Allocate retained operation views in lexical key order. Each view remains
    /// valid after this handle is dropped. Malformed entries refuse
    /// typed enumeration; `value()` remains available for exact inspection.
    ///
    /// # Errors
    /// Returns an interpretation error for an unsupported or malformed version, duplicate members, or an invalid shape in the fields read by this method. This does not require document conformance.
    pub fn operations(&self) -> Result<Vec<OperationView>, InterpretationError> {
        self.interpretable()?;
        self.names()?
            .operations
            .iter()
            .map(|(key, value)| self.operation_view(key, value))
            .collect()
    }
    /// Lexical presentation order; this does not select or rank bindings.
    ///
    /// # Errors
    /// Returns an interpretation error for an unsupported or malformed version, duplicate members, or an invalid shape in the fields read by this method. This does not require document conformance.
    pub fn operation_bindings(&self, key: &str) -> Result<Vec<String>, InterpretationError> {
        self.interpretable()?;
        let index = self.names()?;
        if index.unpaired_binding_key {
            return Err(InterpretationError::UnpairedString);
        }
        if !index.operations.contains_key(key) {
            return Ok(Vec::new());
        }
        Ok(index.bindings.get(key).cloned().unwrap_or_default())
    }
    /// Return `None` when the dependency is absent, otherwise test its kind filter.
    /// Absence of `kinds` accepts all; an empty list matches none but is nonconformant.
    /// Interpretation may refuse the document; this method is not normative proof.
    ///
    /// # Errors
    /// Returns an interpretation error for an unsupported or malformed version, duplicate members, or an invalid shape in the fields read by this method. This does not require document conformance.
    pub fn dependency_accepts_kind(
        &self,
        dependency: &str,
        kind: &str,
    ) -> Result<Option<bool>, InterpretationError> {
        let Some(view) = self.dependency(dependency)? else {
            return Ok(None);
        };
        Ok(Some(match view.kinds()? {
            None => true,
            Some(mut kinds) => kinds.any(|candidate| candidate == kind),
        }))
    }
}
#[derive(Default)]
pub(crate) struct NameIndex {
    operations: BTreeMap<Arc<str>, OperationEntry>,
    // Operation identities share primary bytes; alias occurrences store only indices.
    primary_keys: Vec<Arc<str>>,
    names: HashMap<String, Vec<usize>>,
    bindings: HashMap<String, Vec<String>>,
    // Like the bindings namespace, binding names refuse when a key cannot be text.
    unpaired_binding_key: bool,
}
struct OperationEntry {
    value: JsonValue,
    examples: Arc<NamespaceCache>,
}
impl NameIndex {
    fn build(value: &JsonValue) -> Result<Self, InterpretationError> {
        let mut index = Self::default();
        let operations = value
            .get("operations")
            .ok_or_else(|| InterpretationError::invalid("missing-operations", value.view()))?;
        let operations = operations
            .members()
            .ok_or_else(|| InterpretationError::invalid("invalid-operations-object", operations))?;
        {
            for member in operations {
                let key = member
                    .name
                    .as_str()
                    .ok_or(InterpretationError::UnpairedString)?;
                if member.value.kind() != JsonKind::Object {
                    return Err(InterpretationError::invalid(
                        "invalid-operation-object",
                        member.value,
                    ));
                }
                let primary = Arc::<str>::from(key);
                let id = index.primary_keys.len();
                index.operations.insert(
                    primary.clone(),
                    OperationEntry {
                        value: member.value.to_owned(),
                        examples: Arc::new(OnceLock::new()),
                    },
                );
                index.primary_keys.push(primary);
                index.add_name(key, id);
                if let Some(aliases) = member.value.get("aliases") {
                    let values = aliases.elements().ok_or_else(|| {
                        InterpretationError::invalid("invalid-operation-aliases", aliases)
                    })?;
                    for alias in values {
                        let name = alias.as_str().ok_or_else(|| {
                            InterpretationError::invalid("invalid-operation-alias", alias)
                        })?;
                        index.add_name(name, id);
                    }
                }
            }
        }
        if let Some(bindings) = value.get("bindings").and_then(|v| v.members()) {
            for member in bindings {
                let Some(name) = member.name.as_str() else {
                    index.unpaired_binding_key = true;
                    index.bindings = HashMap::new();
                    break;
                };
                if let Some(operation) = member.value.get("operation").and_then(|v| v.as_str()) {
                    index
                        .bindings
                        .entry(operation.into())
                        .or_default()
                        .push(name.into());
                }
            }
        }
        for names in index.bindings.values_mut() {
            names.sort();
        }
        Ok(index)
    }
    fn add_name(&mut self, name: &str, primary: usize) {
        self.names.entry(name.into()).or_default().push(primary);
    }
}
#[derive(Clone)]
/// Name-resolution result after successful interpretation; missing and ambiguous names are not schema verdicts.
///
/// This closed partition may be exhaustively matched:
/// ```
/// use openbindings::OperationSelection;
/// fn found(selection: &OperationSelection) -> bool {
///     match selection {
///         OperationSelection::Found(_) => true,
///         OperationSelection::Missing | OperationSelection::Ambiguous { .. } => false,
///     }
/// }
/// ```
pub enum OperationSelection {
    /// One retained operation view selected by primary key or alias; the view must still be assessed/prepared as needed.
    Found(OperationView),
    /// No occurrence of the requested name exists.
    Missing,
    /// The name occurs more than once, including a repeated alias in one operation.
    Ambiguous {
        #[doc = "Distinct primary keys in lexical order; a single key can still represent repeated occurrences."]
        candidates: Vec<String>,
    },
}
impl std::fmt::Debug for OperationSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Found(value) => f.debug_tuple("Found").field(value).finish(),
            Self::Missing => f.write_str("Missing"),
            Self::Ambiguous { .. } => f.debug_struct("Ambiguous").finish_non_exhaustive(),
        }
    }
}

/// Immutable retained operation object. Its exact value is not conformance proof.
#[derive(Clone)]
pub struct OperationView {
    document: Document,
    key: Arc<str>,
    value: JsonValue,
    examples: Arc<NamespaceCache>,
}
impl OperationView {
    /// Borrow the selected primary key, even when selection used an alias.
    pub fn key(&self) -> &str {
        &self.key
    }
    /// Borrow the original exact operation object without creating a proof or evaluating schemas.
    pub fn value(&self) -> JsonRef<'_> {
        self.value.view()
    }
    /// The exact input field, without interpreting or preparing its schema.
    /// `None` means absence; JSON null and false remain present exact values.
    /// Only a conformant operation's [`crate::ConformantOperationView::input`]
    /// yields a value contract that can be prepared.
    pub fn input(&self) -> Option<JsonRef<'_>> {
        self.value.get("input")
    }
    /// The exact output field, with the same absence/interpretation distinction
    /// as [`Self::input`]. A present value is not proof of a supported value contract.
    pub fn output(&self) -> Option<JsonRef<'_>> {
        self.value.get("output")
    }
    /// Interpret an optional string description. Unlike exact [`Self::input`]
    /// access, this can refuse a malformed field with its original location.
    ///
    /// # Errors
    /// Returns an interpretation error if the present description is not a string.
    pub fn description(&self) -> Result<Option<JsonStringRef<'_>>, InterpretationError> {
        self.value
            .get("description")
            .map(|v| {
                v.as_string()
                    .ok_or_else(|| InterpretationError::invalid("invalid-operation-description", v))
            })
            .transpose()
    }
    /// Interpret optional string aliases, preserving absence separately from an
    /// empty array. A malformed array or item is a located interpretation error.
    ///
    /// # Errors
    /// Returns an interpretation error if aliases is not an array of scalar strings, including a string with an unpaired UTF-16 unit.
    pub fn aliases(
        &self,
    ) -> Result<Option<impl ExactSizeIterator<Item = &str>>, InterpretationError> {
        self.value
            .get("aliases")
            .map(|v| {
                let values = v
                    .elements()
                    .ok_or_else(|| InterpretationError::invalid("invalid-operation-aliases", v))?;
                for alias in v.elements().expect("checked array") {
                    if alias.as_str().is_none() {
                        return Err(InterpretationError::invalid(
                            "invalid-operation-alias",
                            alias,
                        ));
                    }
                }
                Ok(values.map(|v| v.as_str().expect("checked alias")))
            })
            .transpose()
    }
    /// Allocate binding keys in lexical presentation order; does not rank or invoke bindings.
    ///
    /// # Errors
    /// Returns an interpretation error for an unsupported or malformed version, duplicate members, or an invalid shape in the fields read by this method. This does not require document conformance.
    pub fn bindings(&self) -> Result<Vec<String>, InterpretationError> {
        self.document.operation_bindings(&self.key)
    }
}
impl DocumentValidation {
    /// Borrow complete per-rule evidence and the bounded findings list.
    pub fn report(&self) -> &ValidationReport {
        &self.report
    }
    /// Borrow the admitted snapshot, or `None` when input could not be parsed within admission limits.
    pub fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }
    /// Return a retained proof only for a conformant report; nonconformant and undetermined reports return `None`.
    pub fn conformant(&self) -> Option<Conformant<Document>> {
        if self.report.conclusion == Conformance::Conformant {
            self.document
                .clone()
                .map(|document| Conformant { document })
        } else {
            None
        }
    }
}
impl Conformant<Document> {
    /// The resolution space layering `resources` over this revision. It is
    /// assembled once per live pair: the space without resources lives with the
    /// revision, and a space with a nonempty set lives with that set until
    /// either the set or the revision is dropped. Alternating live sets reuses
    /// each set's space; nothing retains a space beyond its pair.
    pub(crate) fn resolution(&self, resources: &ResourceSet) -> Arc<SchemaSpace> {
        let inner = &self.document.inner;
        if resources.is_empty() {
            return inner
                .resolution
                .get_or_init(|| Arc::new(SchemaSpace::new(&self.document, resources)))
                .clone();
        }
        if let Some(space) = resources.kept_space(inner) {
            return space;
        }
        // Assemble outside every lock; a concurrent preparation's space may win.
        let space =
            resources.keep_space(inner, Arc::new(SchemaSpace::new(&self.document, resources)));
        // The caller holds this revision, so it cannot be dropped before the set
        // learns to release the space with it.
        let mut sets = inner
            .spaces_in
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        sets.retain(|set| set.strong_count() > 0);
        let set = resources.downgrade();
        if !sets.iter().any(|known| known.ptr_eq(&set)) {
            sets.push(set);
        }
        space
    }
    /// Borrow the exact snapshot certified by this proof, for plain fallible reads.
    pub fn as_document(&self) -> &Document {
        &self.document
    }
    /// Consume the proof to obtain an editable document.
    pub fn into_inner(self) -> Document {
        self.document
    }
    /// Borrow the exact bytes certified by this proof, including original formatting.
    pub fn as_bytes(&self) -> &[u8] {
        self.document.as_bytes()
    }
}
/// Assess bytes even when parsing fails. A version refusal is never conformance evidence.
///
/// # Errors
/// Returns a version refusal only for a well-formed declaration outside the supported line. Malformed input and other rule violations are returned as validation evidence, not as this error.
pub fn validate_document(input: impl AsRef<[u8]>) -> Result<DocumentValidation, VersionRefusal> {
    validate_document_with_limits(input, validation_limits())
}
/// Assess bytes as [`validate_document`] does, taking them: a document the
/// validation keeps holds them as its text instead of a copy, so a caller that
/// hands over its buffer holds one copy of the document, not two.
///
/// # Errors
/// Returns a version refusal only for a well-formed declaration outside the supported line. Malformed input and other rule violations are returned as validation evidence, not as this error.
pub fn validate_document_owned(input: Vec<u8>) -> Result<DocumentValidation, VersionRefusal> {
    validate_document_owned_with_limits(input, validation_limits())
}
// What `validate_document` admits: the default bytes and nodes, any nesting.
fn validation_limits() -> JsonLimits {
    JsonLimits {
        max_depth: usize::MAX,
        ..Default::default()
    }
}

/// Assess bytes with caller-selected byte, node and nesting admission limits.
/// Admission precedes version selection: if parsing exceeds a limit, no complete
/// declaration is established and all rules are inconclusive, even for another line.
/// [`validate_document`] uses default byte/node limits and unlimited nesting.
///
/// # Errors
/// Returns a version refusal only for a well-formed declaration outside the supported line. Malformed input and other rule violations are returned as validation evidence, not as this error.
pub fn validate_document_with_limits(
    input: impl AsRef<[u8]>,
    limits: JsonLimits,
) -> Result<DocumentValidation, VersionRefusal> {
    let bytes = input.as_ref();
    match JsonValue::parse_with_limits(bytes, limits) {
        Ok(value) => Document::admitted(value, limits).validate(),
        Err(error) => Ok(refused(bytes, &error, limits)),
    }
}
/// Assess bytes as [`validate_document_with_limits`] does, taking them as
/// [`validate_document_owned`] does.
///
/// # Errors
/// Returns a version refusal only for a well-formed declaration outside the supported line. Malformed input and other rule violations are returned as validation evidence, not as this error.
pub fn validate_document_owned_with_limits(
    input: Vec<u8>,
    limits: JsonLimits,
) -> Result<DocumentValidation, VersionRefusal> {
    match openbindings_internal_json::parse_owned(input, limits) {
        Ok(value) => Document::admitted(value, limits).validate(),
        Err((error, bytes)) => Ok(refused(&bytes, &error, limits)),
    }
}
// The evidence for bytes admission refused: every rule inconclusive past a
// limit, otherwise an OBI-01 violation at the refused byte.
fn refused(bytes: &[u8], error: &InputError, limits: JsonLimits) -> DocumentValidation {
    let mut checks = Checks::new();
    if error.kind == InputErrorKind::Limit {
        let message = input_limit_message(error.code, limits, bytes.len());
        for rule in Rule::ALL {
            checks.mark(
                rule,
                Evidence::Inconclusive,
                "input-limit",
                None,
                message.as_str(),
            );
        }
    } else {
        checks.mark(
            Rule::Obi01,
            Evidence::Violated,
            error.code,
            Some(backend::input_location(bytes, error.byte_offset)),
            parse_error_message(error.code),
        );
        checks.not_applicable_after_json();
    }
    DocumentValidation {
        document: None,
        report: Arc::new(checks.finish()),
    }
}
fn parse_error_message(code: &str) -> &'static str {
    backend::input_error_description(code).unwrap_or("invalid JSON text")
}
fn version_refusal(value: &JsonValue) -> Option<VersionRefusal> {
    let mut declarations = value
        .view()
        .members()?
        .filter(|m| m.name.as_str() == Some("openbindings"));
    let declared = declarations.next()?.value.as_str()?;
    if declarations.next().is_some() {
        return None;
    }
    VersionRefusal::new(declared)
}
// Which admission limit stopped parsing, and how to validate past it, in Rust
// and in TypeScript. Parsing reports an exceeded limit as `byte-limit`,
// `node-limit` or `depth-limit`.
fn input_limit_message(code: &str, limits: JsonLimits, bytes: usize) -> String {
    let reached = match code {
        "byte-limit" => format!(
            "the document's {bytes} bytes exceed the {}-byte admission limit",
            limits.max_bytes
        ),
        "node-limit" => format!(
            "the document holds more than the {} nodes admitted",
            limits.max_nodes.min(u32::MAX as usize)
        ),
        _ => format!(
            "the document nests containers deeper than the {} levels admitted",
            limits.max_depth
        ),
    };
    let (rust, typescript) = backend::limit_setting(code).unwrap_or(("max_depth", "maxDepth"));
    // The route works wherever a document is validated, discovered bodies and
    // publication bytes included: validate it again with a larger limit.
    format!(
        "{reached}; validate it with validate_document_with_limits and a larger \
         JsonLimits::{rust} (validateDocument with a larger limits.{typescript} in TypeScript)"
    )
}
const MAX_FINDINGS: usize = 4096;
const MAX_FINDING_POINTER_BYTES: usize = 8 * 1024 * 1024;
struct Checks<'a> {
    evidence: [Evidence; 13],
    findings: Vec<Finding>,
    pending_locations: Vec<(usize, JsonRef<'a>)>,
    retained_at: HashMap<usize, Vec<usize>>,
    retained_direct: Vec<usize>,
    pointer_bytes: usize,
    // Once one location's pointer has not fitted the allowance, every later
    // one is read from a table measured in one pass, so that many findings
    // deep in a large document are not each measured by a walk to the root.
    pointer_sizes: Option<backend::LocationPointerSizes>,
    truncated: bool,
}
impl<'a> Checks<'a> {
    fn new() -> Self {
        Self {
            evidence: [Evidence::Met; 13],
            findings: Vec::new(),
            pending_locations: Vec::new(),
            retained_at: HashMap::new(),
            retained_direct: Vec::new(),
            pointer_bytes: MAX_FINDING_POINTER_BYTES,
            pointer_sizes: None,
            truncated: false,
        }
    }
    fn update_evidence(&mut self, rule: Rule, status: Evidence) {
        let rule = rule as usize;
        if status == Evidence::Violated || self.evidence[rule] != Evidence::Violated {
            self.evidence[rule] = status;
        }
    }
    fn same_finding(
        &self,
        index: usize,
        rule: Rule,
        status: Evidence,
        code: &str,
        message: &str,
    ) -> bool {
        let finding = &self.findings[index];
        finding.rule == rule.id()
            && finding.status == status
            && finding.code == code
            && finding.message == message
    }
    fn contains_at(
        &self,
        rule: Rule,
        status: Evidence,
        at: JsonRef<'_>,
        code: &str,
        message: &str,
    ) -> bool {
        self.retained_at
            .get(&fixed_schema::occurrence(at))
            .is_some_and(|indices| {
                indices
                    .iter()
                    .any(|&index| self.same_finding(index, rule, status, code, message))
            })
    }
    fn mark(
        &mut self,
        rule: Rule,
        status: Evidence,
        code: &'static str,
        location: Option<SourceLocation>,
        message: impl Into<String>,
    ) {
        self.update_evidence(rule, status);
        let message = message.into();
        if self.retained_direct.iter().any(|&index| {
            self.same_finding(index, rule, status, code, &message)
                && self.findings[index].location == location
        }) {
            return;
        }
        if self.findings.len() < MAX_FINDINGS {
            self.retained_direct.push(self.findings.len());
            self.findings.push(Finding {
                rule: rule.id(),
                status,
                code,
                location,
                message,
            });
        } else {
            self.truncated = true;
        }
    }
    // A message becomes owned text only when its finding is retained.
    fn mark_at<M: AsRef<str> + Into<String>>(
        &mut self,
        rule: Rule,
        status: Evidence,
        at: JsonRef<'a>,
        code: &'static str,
        message: M,
    ) {
        self.update_evidence(rule, status);
        // Identity is checked before either allowance. Only retained findings
        // have index entries, so omitted data never grows an all-seen set.
        if self.contains_at(rule, status, at, code, message.as_ref()) {
            return;
        }
        if self.findings.len() >= MAX_FINDINGS {
            self.truncated = true;
            return;
        }
        let bytes = match &self.pointer_sizes {
            Some(sizes) => sizes.size(at, self.pointer_bytes),
            None => backend::location_pointer_size(at, self.pointer_bytes),
        };
        let Some(bytes) = bytes else {
            if self.pointer_sizes.is_none() {
                self.pointer_sizes = Some(backend::LocationPointerSizes::new(
                    at,
                    MAX_FINDING_POINTER_BYTES + 1,
                ));
            }
            self.truncated = true;
            return;
        };
        self.pointer_bytes -= bytes;
        let index = self.findings.len();
        self.pending_locations.push((index, at));
        self.retained_at
            .entry(fixed_schema::occurrence(at))
            .or_default()
            .push(index);
        self.findings.push(Finding {
            rule: rule.id(),
            status,
            code,
            location: None,
            message: message.into(),
        });
    }
    fn violation<M: AsRef<str> + Into<String>>(
        &mut self,
        rule: Rule,
        at: JsonRef<'a>,
        code: &'static str,
        message: M,
    ) {
        self.mark_at(rule, Evidence::Violated, at, code, message);
    }
    fn not_applicable_after_json(&mut self) {
        self.evidence[Rule::Obi02 as usize..].fill(Evidence::NotApplicable);
    }
    fn finish(mut self) -> ValidationReport {
        let nodes: Vec<_> = self.pending_locations.iter().map(|(_, at)| *at).collect();
        for ((index, _), location) in self
            .pending_locations
            .into_iter()
            .zip(backend::locations(&nodes))
        {
            self.findings[index].location = Some(location);
        }
        let conclusion = if self.evidence.contains(&Evidence::Violated) {
            Conformance::NonConformant
        } else if self.evidence.contains(&Evidence::Inconclusive) {
            Conformance::Undetermined
        } else {
            Conformance::Conformant
        };
        ValidationReport {
            release: APPLIED_SPEC_RELEASE,
            revision: APPLIED_SPEC_REVISION,
            conclusion,
            evidence: DOCUMENT_RULES.into_iter().zip(self.evidence).collect(),
            findings: self.findings,
            findings_truncated: self.truncated,
        }
    }
    fn fixed(&mut self, value: JsonRef<'a>, rule: Rule, is_meta: bool) {
        let checked = fixed_schema::check_with_retained(
            value,
            is_meta,
            MAX_FINDINGS.saturating_sub(self.findings.len()),
            |at, message| {
                self.contains_at(rule, Evidence::Violated, at, "schema-mismatch", message)
            },
        );
        self.fixed_checked(value, rule, checked);
    }
    // OBI-10 for the next indexed schema, checked through the index, which
    // records whether every indexed schema satisfies the meta-schemas.
    fn meta_fixed(&mut self, meta: &mut schema_index::MetaChecks<'a>) {
        let rule = Rule::Obi10;
        let (value, checked) = meta.check_next(
            MAX_FINDINGS.saturating_sub(self.findings.len()),
            |at, message| {
                self.contains_at(rule, Evidence::Violated, at, "schema-mismatch", message)
            },
        );
        self.fixed_checked(value, rule, checked);
    }
    fn fixed_checked(
        &mut self,
        value: JsonRef<'a>,
        rule: Rule,
        checked: Result<fixed_schema::Problems<'a>, String>,
    ) {
        match checked {
            Ok(problems) => {
                self.truncated |= problems.truncated;
                if problems.violated {
                    self.evidence[rule as usize] = Evidence::Violated;
                }
                for problem in problems.entries {
                    self.mark_at(
                        rule,
                        Evidence::Violated,
                        problem.at,
                        "schema-mismatch",
                        problem.message,
                    );
                }
            }
            Err(reason) => self.mark_at(
                rule,
                Evidence::Inconclusive,
                value,
                "fixed-schema-limit-or-failure",
                reason,
            ),
        }
    }
}
const NAME_GRAMMAR_MESSAGE: &str = "a name must be a nonempty ASCII string: start with a letter, digit or underscore; then use letters, digits, underscores, dots or hyphens";
/// Test the normative name grammar: ASCII letter, digit or underscore first, followed by ASCII letters, digits, underscores, dots or hyphens. No Unicode normalization or case folding is performed.
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().enumerate().all(|(i, c)| {
            c.is_ascii_alphanumeric() || c == b'_' || (i > 0 && (c == b'-' || c == b'.'))
        })
}
fn assess_value(document: &Document) -> Result<ValidationReport, VersionRefusal> {
    let value = document.value();
    if let Some(refusal) = version_refusal(value) {
        return Err(refusal);
    }
    let mut c = Checks::new();
    // Version declaration follows §8.1's JSON grammar, independently of this
    // line's OBI-01 exclusions. OBI-02 through OBI-13 require OBI-01 (§10).
    for at in backend::unpaired_strings(value).take(MAX_FINDINGS + 1) {
        c.violation(
            Rule::Obi01,
            at,
            "unpaired-surrogate",
            "member name or string value holds an unpaired surrogate escape",
        );
    }
    let duplicates = backend::duplicate_member_names(value);
    if duplicates.len() != 0 {
        c.truncated |= duplicates.len() > MAX_FINDINGS.saturating_sub(c.findings.len());
        for at in duplicates.take(MAX_FINDINGS) {
            c.mark_at(
                Rule::Obi01,
                Evidence::Violated,
                at,
                "duplicate-member",
                "object repeats a decoded member name",
            );
        }
    }
    if c.evidence[Rule::Obi01 as usize] == Evidence::Violated {
        c.not_applicable_after_json();
        return Ok(c.finish());
    }
    match value.get("openbindings") {
        Some(v)
            if v.as_str()
                .is_some_and(|s| check_version(s) != VersionDecision::Malformed) => {}
        Some(v) => c.violation(
            Rule::Obi03,
            v,
            "semver",
            "openbindings must be a SemVer 2.0.0 string",
        ),
        None => c.violation(
            Rule::Obi03,
            value.view(),
            "missing-version",
            "required openbindings member is absent",
        ),
    }
    c.fixed(value.view(), Rule::Obi02, false);
    let operations = value.get("operations");
    let sources = value.get("sources");
    for map in [
        "schemas",
        "operations",
        "dependencies",
        "sources",
        "bindings",
    ] {
        if let Some(entries) = value.get(map).and_then(|v| v.members()) {
            for entry in entries {
                if !entry.name.as_str().is_some_and(is_valid_name) {
                    c.violation(
                        Rule::Obi04,
                        entry.name,
                        "name-grammar",
                        NAME_GRAMMAR_MESSAGE,
                    );
                }
            }
        }
    }
    if let Some(operations) = operations
        && let Some(entries) = operations.members()
    {
        // Keys and aliases share one namespace. OBI-01 holds here, so no member
        // name repeats and every string is scalar text: an alias repeats a name
        // exactly when it is a key, found through the map's own index, or an
        // earlier alias, found by sorting the aliases by text.
        let aliases: Vec<u32> = operations
            .members()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.value.get("aliases").and_then(|v| v.elements()))
            .flatten()
            .map(|alias| backend::ref_id(alias) as u32)
            .collect();
        let text =
            |at: u32| backend::node(value, aliases[at as usize] as usize).and_then(|a| a.as_str());
        // A stable sort keeps each text's aliases in document order.
        let mut by_text: Vec<u32> = (0..aliases.len() as u32).collect();
        by_text.sort_by_key(|&at| text(at));
        let mut repeated = vec![false; aliases.len()];
        for run in by_text.chunk_by(|&a, &b| text(a) == text(b)) {
            if text(run[0]).is_some() {
                for &later in &run[1..] {
                    repeated[later as usize] = true;
                }
            }
        }
        drop(by_text);
        drop(aliases);
        let mut ordinal = 0;
        for entry in entries {
            if let Some(list) = entry.value.get("aliases").and_then(|v| v.elements()) {
                for alias in list {
                    let earlier = repeated[ordinal];
                    ordinal += 1;
                    if !alias.as_str().is_some_and(is_valid_name) {
                        c.violation(Rule::Obi04, alias, "name-grammar", NAME_GRAMMAR_MESSAGE);
                    }
                    let Some(name) = alias.as_str() else {
                        continue;
                    };
                    if earlier || operations.get(name).is_some() {
                        c.violation(
                            Rule::Obi05,
                            alias,
                            "duplicate-operation-name",
                            "operation keys and aliases must be distinct",
                        );
                    }
                }
            }
            if let Some(examples) = entry.value.get("examples").and_then(|v| v.members()) {
                for example in examples {
                    if !example.name.as_str().is_some_and(is_valid_name) {
                        c.violation(
                            Rule::Obi04,
                            example.name,
                            "name-grammar",
                            NAME_GRAMMAR_MESSAGE,
                        );
                    }
                }
            }
        }
    }
    // References name primary map keys, including keys whose values are
    // independently malformed. A large map's lookup uses its member index, so
    // checking many references stays linear. Every string is scalar here.
    for (map, field, targets, rule) in [
        ("bindings", "operation", operations, Rule::Obi06),
        ("bindings", "source", sources, Rule::Obi07),
        ("dependencies", "operation", operations, Rule::Obi08),
    ] {
        if let Some(entries) = value.get(map).and_then(|v| v.members()) {
            for entry in entries {
                if let Some(reference) = entry.value.get(field)
                    && reference
                        .as_str()
                        .is_none_or(|name| targets.and_then(|map| map.get(name)).is_none())
                {
                    c.violation(
                        rule,
                        reference,
                        if field == "source" {
                            "unknown-source"
                        } else {
                            "unknown-operation"
                        },
                        "reference must name an existing primary map key",
                    );
                }
            }
        }
    }
    let schemas = document.schema_index();
    // The index visits each contained schema occurrence once. Equal text at
    // different positions must still be checked at each original location.
    let mut meta = schemas.meta_checks();
    for (index, node) in schemas.nodes.iter().enumerate() {
        let schema = schemas.value(node);
        c.meta_fixed(&mut meta);
        if let Some(dialect) = schema.get("$schema")
            && !matches!(
                dialect.as_str(),
                Some(DIALECT) | Some("https://json-schema.org/draft/2020-12/schema#")
            )
        {
            c.violation(
                Rule::Obi09,
                dialect,
                "schema-dialect",
                "contained schemas must name the 2020-12 dialect",
            );
        }
        if schemas.obi_position(index) {
            if let Some(id) = schema.get("$id") {
                if !id.as_str().is_some_and(uri::absolute) {
                    c.violation(
                        Rule::Obi11,
                        id,
                        "absolute-schema-id",
                        "a document-resource $id must be an absolute URI",
                    );
                }
                continue;
            }
            for keyword in ["$ref", "$dynamicRef"] {
                if let Some(reference) = schema.get(keyword) {
                    let Some(text) = reference.as_str() else {
                        c.violation(
                            Rule::Obi11,
                            reference,
                            "reference-type",
                            "schema reference must be a URI-reference string",
                        );
                        continue;
                    };
                    if !uri::valid(text)
                        || (!uri::absolute(text) && !text.is_empty() && !text.starts_with('#'))
                    {
                        c.violation(
                            Rule::Obi11,
                            reference,
                            "reference-form",
                            "document-resource references must be absolute or same-document",
                        );
                        continue;
                    }
                    if (text.is_empty() || text.starts_with('#'))
                        && matches!(schemas.same_document(value, text), SameDocument::Missing)
                    {
                        c.violation(
                            Rule::Obi12,
                            reference,
                            "reference-target",
                            "same-document reference identifies no schema at an OBI position",
                        );
                    }
                }
            }
        }
    }
    meta.finish();
    for declarations in schemas.anchor_declarations() {
        if declarations[0].resource == 0 && declarations.len() > 1 {
            for anchor in declarations {
                c.violation(
                    Rule::Obi13,
                    schemas.anchor_token(anchor),
                    "duplicate-anchor",
                    "plain name is declared more than once in the document resource",
                );
            }
        }
    }
    for declarations in schemas.repeated_identifiers() {
        for &(_, node) in declarations {
            c.violation(
                Rule::Obi13,
                schemas
                    .value(&schemas.nodes[node as usize])
                    .get("$id")
                    .unwrap(),
                "duplicate-schema-id",
                "schemas declare the same compared identifier",
            );
        }
    }
    Ok(c.finish())
}

impl std::fmt::Debug for OperationView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OperationView")
            .field("value", &self.value)
            .finish_non_exhaustive()
    }
}
#[cfg(test)]
mod name_index_storage_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn alias_occurrences_share_primary_storage_without_scaling_its_bytes() {
        // Small, deterministic representation check, not a peak-memory benchmark.
        let primary = "p".repeat(4096);
        for count in [0, 1, 64, 256] {
            let aliases: Vec<_> = (0..count).map(|i| format!("a{i}")).collect();
            let source = serde_json::json!({
                "openbindings": "0.2.0",
                "operations": { &primary: { "aliases": aliases } },
            });
            let document = Document::parse(source.to_string()).unwrap();
            let index = document.names().unwrap();
            assert_eq!(index.primary_keys.len(), 1);
            let key = &index.primary_keys[0];
            assert!(Arc::ptr_eq(index.operations.keys().next().unwrap(), key));
            assert_eq!(index.names.len(), count + 1);
            let occurrences: Vec<_> = index.names.values().flatten().copied().collect();
            assert_eq!(occurrences.len(), count + 1);
            assert!(occurrences.iter().all(|&id| id == 0));

            // Count distinct retained primary allocations reached by all index
            // owners/occurrences, plus its independent namespace lookup string.
            let mut addresses = HashSet::new();
            let primary_bytes: usize = index
                .operations
                .keys()
                .chain(&index.primary_keys)
                .chain(occurrences.iter().map(|&id| &index.primary_keys[id]))
                .filter(|key| addresses.insert(Arc::as_ptr(key)))
                .map(|key| key.len())
                .sum();
            assert_eq!(addresses.len(), 1);
            assert_eq!(primary_bytes, primary.len());
            let lookup_bytes = index.names.get_key_value(&primary).unwrap().0.len();
            assert_eq!(primary_bytes + lookup_bytes, 2 * primary.len());

            let selected = aliases.last().map(String::as_str).unwrap_or(&primary);
            let OperationSelection::Found(operation) =
                document.resolve_operation(selected).unwrap()
            else {
                panic!("every unique alias selects its primary")
            };
            assert_eq!(operation.key(), primary);
        }
    }

    #[test]
    fn repeated_aliases_stay_ambiguous_with_distinct_lexical_candidates() {
        let primary = "p".repeat(4096);
        let mut aliases = vec!["shared"; 256];
        aliases.push(&primary); // primary/alias collision is another occurrence.
        let source = serde_json::json!({
            "openbindings": "0.2.0",
            "operations": {
                &primary: { "aliases": aliases },
                "z": { "aliases": ["shared"] },
                "a": { "aliases": ["shared"] },
            },
        });
        let document = Document::parse(source.to_string()).unwrap();
        let OperationSelection::Ambiguous { candidates } =
            document.resolve_operation("shared").unwrap()
        else {
            panic!("repeated declarations stay ambiguous")
        };
        assert_eq!(
            candidates,
            ["a".to_owned(), primary.clone(), "z".to_owned()]
        );
        let OperationSelection::Ambiguous { candidates } =
            document.resolve_operation(&primary).unwrap()
        else {
            panic!("primary/alias collision stays ambiguous")
        };
        assert_eq!(candidates, [primary]);
    }
}

#[cfg(test)]
mod finding_identity_tests {
    use super::*;
    #[test]
    fn each_rule_names_the_identifier_it_indexes() {
        for (index, rule) in Rule::ALL.into_iter().enumerate() {
            assert_eq!(rule as usize, index);
            let name = format!("{rule:?}").to_uppercase().replace("OBI", "OBI-");
            assert_eq!(rule.id(), name);
        }
    }
    #[test]
    fn final_dedup_precedes_count_and_pointer_caps_and_preserves_evidence() {
        let value = JsonValue::parse(r#"{"a":0,"b":0}"#).unwrap();
        let a = value.get("a").unwrap();
        let b = value.get("b").unwrap();
        let mut checks = Checks::new();
        checks.pointer_bytes = 2;
        checks.violation(Rule::Obi01, a, "test", "first");
        checks.violation(Rule::Obi01, a, "test", "first");
        assert_eq!(checks.pointer_bytes, 0);
        assert_eq!(checks.findings.len(), 1);
        assert!(!checks.truncated);
        checks.violation(Rule::Obi02, b, "test", "distinct");
        assert!(checks.truncated);
        assert_eq!(checks.retained_at.len(), 1);
        assert_eq!(checks.finish().evidence["OBI-02"], Evidence::Violated);

        let mut checks = Checks::new();
        for i in 0..MAX_FINDINGS {
            checks.violation(Rule::Obi01, a, "test", format!("message {i}"));
        }
        checks.violation(Rule::Obi01, a, "test", "message 0");
        assert!(!checks.truncated);
        assert_eq!(
            checks.retained_at.values().map(Vec::len).sum::<usize>(),
            MAX_FINDINGS
        );
        checks.violation(Rule::Obi02, a, "test", "message 0");
        assert!(checks.truncated);
        assert_eq!(checks.evidence[Rule::Obi02 as usize], Evidence::Violated);
    }

    #[test]
    fn final_identity_includes_rule_status_code_message_and_original_occurrence() {
        let value = JsonValue::parse(r#"{"k":0,"k":0,"k":0}"#).unwrap();
        let nodes: Vec<_> = backend::duplicate_member_names(&value).collect();
        let mut checks = Checks::new();
        for node in &nodes {
            checks.violation(Rule::Obi01, *node, "test", "same");
        }
        checks.violation(Rule::Obi02, nodes[0], "test", "same");
        checks.mark_at(
            Rule::Obi01,
            Evidence::Inconclusive,
            nodes[0],
            "test",
            "same",
        );
        checks.violation(Rule::Obi01, nodes[0], "other", "same");
        checks.violation(Rule::Obi01, nodes[0], "test", "other");
        checks.violation(Rule::Obi01, nodes[0], "test", "same");
        let report = checks.finish();
        assert_eq!(report.findings.len(), 6);
        assert_eq!(
            report.findings[0].location.as_ref().unwrap().pointer,
            report.findings[1].location.as_ref().unwrap().pointer
        );
        assert_ne!(
            report.findings[0].location.as_ref().unwrap().byte_offset,
            report.findings[1].location.as_ref().unwrap().byte_offset
        );
        assert_eq!(report.evidence["OBI-01"], Evidence::Violated);
        assert!(!report.findings_truncated);
    }

    #[test]
    fn a_later_inconclusive_result_keeps_an_established_violation() {
        let value = JsonValue::parse(r#"{"a":0}"#).unwrap();
        let a = value.get("a").unwrap();
        let mut checks = Checks::new();
        checks.violation(Rule::Obi01, a, "test", "violated");
        checks.mark_at(Rule::Obi01, Evidence::Inconclusive, a, "test", "undecided");
        checks.mark(
            Rule::Obi01,
            Evidence::Inconclusive,
            "test",
            None,
            "undecided",
        );
        // An inconclusive result before a violation does not hide it either.
        checks.mark(
            Rule::Obi02,
            Evidence::Inconclusive,
            "test",
            None,
            "undecided",
        );
        checks.violation(Rule::Obi02, a, "test", "violated");
        let report = checks.finish();
        assert_eq!(report.evidence["OBI-01"], Evidence::Violated);
        assert_eq!(report.evidence["OBI-02"], Evidence::Violated);
        assert_eq!(report.conclusion, Conformance::NonConformant);
        assert_eq!(report.findings.len(), 5);
    }

    #[test]
    fn direct_findings_with_equal_text_are_distinct_at_distinct_locations() {
        let source = b"{}\n{}";
        let first = backend::input_location(source, 0);
        let second = backend::input_location(source, 3);
        let mut checks = Checks::new();
        for location in [&first, &second, &first, &second] {
            checks.mark(
                Rule::Obi01,
                Evidence::Violated,
                "value",
                Some(location.clone()),
                "same",
            );
        }
        checks.mark(Rule::Obi01, Evidence::Violated, "value", None, "same");
        checks.mark(Rule::Obi01, Evidence::Violated, "value", None, "same");
        let report = checks.finish();
        let locations: Vec<_> = report.findings.iter().map(|f| f.location.clone()).collect();
        assert_eq!(locations, [Some(first), Some(second), None]);
    }

    #[test]
    fn direct_findings_beyond_the_cap_are_counted_as_truncated() {
        let mut checks = Checks::new();
        for i in 0..MAX_FINDINGS {
            checks.mark(
                Rule::Obi01,
                Evidence::Violated,
                "test",
                None,
                format!("distinct {i}"),
            );
        }
        assert!(!checks.truncated);
        checks.mark(
            Rule::Obi02,
            Evidence::Inconclusive,
            "test",
            None,
            "one more",
        );
        assert!(checks.truncated);
        let report = checks.finish();
        assert_eq!(report.findings.len(), MAX_FINDINGS);
        assert!(report.findings_truncated);
        // The omitted finding's evidence still counts.
        assert_eq!(report.evidence["OBI-02"], Evidence::Inconclusive);
    }

    #[test]
    fn fixed_stream_excludes_already_retained_findings_before_intermediate_cap() {
        let value = JsonValue::parse("null").unwrap();
        let mut checks = Checks::new();
        checks.fixed(value.view(), Rule::Obi03, true);
        let fixed_count = checks.findings.len();
        for i in fixed_count..MAX_FINDINGS {
            checks.mark(
                Rule::Obi01,
                Evidence::Violated,
                "padding",
                None,
                format!("distinct {i}"),
            );
        }
        checks.fixed(value.view(), Rule::Obi03, true);
        assert_eq!(checks.findings.len(), MAX_FINDINGS);
        assert!(!checks.truncated);
        assert_eq!(checks.evidence[Rule::Obi03 as usize], Evidence::Violated);
    }
}

#[cfg(test)]
mod resolution_reuse {
    use super::*;
    use crate::schema_space::ASSEMBLED;
    use std::sync::Weak;
    /// Projects the core resolution space on every preparation, as custom
    /// evaluators may, and retains its request, as diagnostics-bearing
    /// prepared schemas do.
    struct Projecting;
    struct Retaining(#[allow(dead_code)] SchemaRequest);
    impl SchemaEvaluator for Projecting {
        fn prepare(
            &self,
            request: &SchemaRequest,
            control: &dyn WorkControl,
        ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
            request.evaluation_program(control)?;
            Ok(Arc::new(Retaining(request.clone())))
        }
    }
    /// Never asks for the core projection, like the default evaluator.
    struct Accepting;
    impl SchemaEvaluator for Accepting {
        fn prepare(
            &self,
            request: &SchemaRequest,
            _: &dyn WorkControl,
        ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
            Ok(Arc::new(Retaining(request.clone())))
        }
    }
    impl PreparedSchema for Retaining {
        fn validate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
            ValueOutcome::Satisfies
        }
    }
    fn proof() -> Conformant<Document> {
        Document::parse(
            r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/item"},"output":true}}}"#,
        )
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
    }
    fn snapshot(minimum: u32) -> ResourceSet {
        ResourceSet::new([SchemaResource {
            uri: "https://example.test/item".into(),
            document: JsonValue::parse(format!(r#"{{"type":"integer","minimum":{minimum}}}"#))
                .unwrap(),
        }])
        .unwrap()
    }
    fn assembled() -> usize {
        ASSEMBLED.with(std::cell::Cell::get)
    }
    /// The space kept for a live pair, without assembling one.
    fn kept(proof: &Conformant<Document>, set: &ResourceSet) -> Option<Weak<SchemaSpace>> {
        set.kept_space(&proof.document.inner)
            .map(|space| Arc::downgrade(&space))
    }

    #[test]
    fn evaluators_that_never_project_assemble_no_space() {
        let proof = proof();
        let operation = proof.resolve_operation("op").unwrap();
        let set = snapshot(1);
        let before = assembled();
        drop(
            operation
                .input()
                .unwrap()
                .prepare_with(&Accepting, &set)
                .unwrap(),
        );
        drop(operation.output().unwrap().prepare(&Accepting).unwrap());
        assert_eq!(assembled(), before);
        assert!(proof.document.inner.resolution.get().is_none());
        assert!(kept(&proof, &set).is_none());
    }

    #[test]
    fn repeated_and_alternating_sets_reuse_one_space_per_live_pair() {
        let proof = proof();
        let operation = proof.resolve_operation("op").unwrap();
        let (input, output) = (operation.input().unwrap(), operation.output().unwrap());
        let (first, second) = (snapshot(1), snapshot(2));
        let before = assembled();
        for _ in 0..3 {
            for set in [&first, &second, &first.clone()] {
                drop(input.prepare_with(&Projecting, set).unwrap());
                drop(output.prepare_with(&Projecting, set).unwrap());
            }
            drop(output.prepare(&Projecting).unwrap());
            drop(
                output
                    .prepare_with(&Projecting, &ResourceSet::default())
                    .unwrap(),
            );
        }
        // One space per set, plus one without resources, however preparations alternate.
        assert_eq!(assembled() - before, 3);
        let (a, b) = (
            kept(&proof, &first).unwrap(),
            kept(&proof, &second).unwrap(),
        );
        assert!(!a.ptr_eq(&b));
    }

    #[test]
    fn dropping_a_set_or_every_document_owner_releases_its_space() {
        let proof = proof();
        let operation = proof.resolve_operation("op").unwrap();
        let input = operation.input().unwrap();
        let (first, second) = (snapshot(1), snapshot(2));
        drop(input.prepare_with(&Projecting, &first).unwrap());
        drop(input.prepare_with(&Projecting, &second).unwrap());
        let (a, b) = (
            kept(&proof, &first).unwrap(),
            kept(&proof, &second).unwrap(),
        );
        // The set goes; its space goes with it while the document lives on, and
        // the document stops tracking it.
        drop(first);
        assert!(a.upgrade().is_none());
        assert!(b.upgrade().is_some());
        assert_eq!(proof.document.inner.spaces_in.lock().unwrap().len(), 1);
        // Every document owner goes; the set it outlives no longer keeps its space.
        drop(operation);
        drop(proof);
        assert!(b.upgrade().is_none());
        assert_eq!(second.kept_spaces(), 0);
    }

    #[test]
    fn a_prepared_value_contract_keeps_no_space_of_a_set_it_never_received() {
        let proof = proof();
        let operation = proof.resolve_operation("op").unwrap();
        // Prepared without resources and retaining its request, hence the document.
        let keep = operation.output().unwrap().prepare(&Projecting).unwrap();
        let without = Arc::downgrade(proof.document.inner.resolution.get().unwrap());
        let other = snapshot(1);
        let prepared = operation
            .input()
            .unwrap()
            .prepare_with(&Projecting, &other)
            .unwrap();
        let space = kept(&proof, &other).unwrap();
        drop((prepared, other, operation, proof));
        // The document lives on through `keep`; the other set's space does not.
        assert!(space.upgrade().is_none());
        assert!(without.upgrade().is_some());
        drop(keep);
        assert!(without.upgrade().is_none());
    }

    #[test]
    fn concurrent_preparations_project_their_own_set() {
        /// Fails preparation when the projection lacks the request's own snapshot,
        /// recognized by its `minimum` member.
        struct Checking;
        impl SchemaEvaluator for Checking {
            fn prepare(
                &self,
                request: &SchemaRequest,
                control: &dyn WorkControl,
            ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
                let program = request.evaluation_program(control)?;
                if let Some(resource) = request.supplied_resources().iter().next() {
                    let text = resource.document.text();
                    let marker = text[text.find("\"minimum\":").unwrap()..].trim_end_matches('}');
                    if !program
                        .resources()
                        .iter()
                        .any(|r| r.document.text().contains(marker))
                    {
                        return Err(NoVerdict::new(
                            NoVerdictReason::EvaluatorFailure,
                            "crossed-sets",
                            "the projection carries another set's snapshot",
                        ));
                    }
                }
                Ok(Arc::new(Retaining(request.clone())))
            }
        }
        let proof = proof();
        let sets: Vec<_> = (0..6).map(snapshot).collect();
        std::thread::scope(|scope| {
            for thread in 0..6 {
                let (proof, sets) = (&proof, &sets);
                scope.spawn(move || {
                    let operation = proof.resolve_operation("op").unwrap();
                    let (input, output) = (operation.input().unwrap(), operation.output().unwrap());
                    for i in 0..60 {
                        let set = &sets[(thread + i) % sets.len()];
                        assert!(input.prepare_with(&Checking, set).is_ok());
                        if i % 5 == 0 {
                            assert!(output.prepare(&Checking).is_ok());
                        }
                    }
                });
            }
        });
        assert!(sets.iter().all(|set| set.kept_spaces() == 1));
    }
}
