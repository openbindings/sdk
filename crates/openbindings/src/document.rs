//! Immutable document snapshots and complete conformance evidence.
use crate::{
    fixed_schema,
    schema_index::{DIALECT, SameDocument, SchemaIndex},
    *,
};
use openbindings_internal_json::backend;
use serde::Serialize;
mod read_views;
use read_views::NamespaceCache;
pub use read_views::{BindingView, DependencyView, ExampleView, SourceView};
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    sync::{Arc, OnceLock},
};

/// Stable rule identifiers OBI-01 through OBI-13, in specification order.
pub const DOCUMENT_RULES: [&str; 13] = [
    "OBI-01", "OBI-02", "OBI-03", "OBI-04", "OBI-05", "OBI-06", "OBI-07", "OBI-08", "OBI-09",
    "OBI-10", "OBI-11", "OBI-12", "OBI-13",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Evidence for one normative document rule. This closed partition may be exhaustively matched; it is separate from explanatory findings.
pub enum Evidence {
    /// The rule was established for this snapshot.
    Satisfied,
    /// A rule violation was established, even if other work is inconclusive.
    Violated,
    /// The implementation could not establish satisfaction or violation.
    Inconclusive,
    /// The rule does not apply after an earlier prerequisite fails.
    NotApplicable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Overall normative document conclusion. This is a closed semantic partition, not a schema-instance validation result.
pub enum Conformance {
    /// Every applicable document rule was established; a validated document can be obtained.
    Conformant,
    /// At least one document rule was established as violated.
    NonConformant,
    /// No violation was established, but evidence is insufficient to prove conformance.
    Undetermined,
}
#[derive(Clone, Debug, Serialize)]
/// One explanatory normative-rule finding. Findings may be bounded; the report's independent rule evidence determines its conclusion.
pub struct Finding {
    /// Normative rule identifier, such as `OBI-01`.
    pub rule: &'static str,
    /// Evidence expressed by this finding, not necessarily the final aggregate for its rule.
    pub status: Evidence,
    /// Stable machine-readable diagnostic identifier; prefer it over message matching.
    pub code: &'static str,
    /// Original-source coordinates when available. Pointers are data and must be escaped for display; byte columns are not UTF-16 editor columns.
    pub location: Option<SourceLocation>,
    /// Human-facing explanation; display as text and use `rule`, `code` and `status` for logic.
    pub message: String,
}
#[derive(Clone, Debug, Serialize)]
/// Assessment of all normative document rules at the pinned specification revision. At most 4096 findings and 8 MiB of aggregate generated-pointer UTF-8 bytes are retained. Omitted findings set `findings_truncated`; rule evidence and the conclusion remain independent of presentation caps. Retained coordinates always refer to original source.
pub struct ConformanceReport {
    /// Applied specification release, independent of package version.
    pub release: &'static str,
    /// Exact applied specification Git revision.
    pub revision: &'static str,
    /// Identifier of the assessment policy used for this report.
    pub policy: &'static str,
    /// Aggregate normative conclusion derived from all rule evidence.
    pub conclusion: Conformance,
    /// One entry for every [`DOCUMENT_RULES`] identifier, including inconclusive and not-applicable rules.
    pub evidence: BTreeMap<&'static str, Evidence>,
    /// Retained explanatory findings; an empty or truncated list alone does not prove conformance.
    pub findings: Vec<Finding>,
    /// True when findings exceed the 4096-entry or 8 MiB aggregate generated-pointer byte cap. Omission never changes rule evidence or fabricates a shortened source pointer.
    pub findings_truncated: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
/// A well-formed declared version outside this implementation's supported line. This is separate from conformance evidence.
pub struct VersionRefusal {
    /// Original declared version string.
    pub declared: String,
    /// Supported specification line used for the refusal.
    pub supported: &'static str,
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
/// Immutable retained exact JSON snapshot, without a conformance claim. Clones share storage and lazily cached interpretation/assessment. Dropping all owners releases storage; it does not promise an immediate process RSS decrease.
pub struct ParsedDocument {
    pub(crate) inner: Arc<DocumentInner>,
}
pub(crate) struct DocumentInner {
    pub value: JsonValue,
    pub schemas: OnceLock<SchemaIndex>,
    assessment: OnceLock<Result<Arc<ConformanceReport>, VersionRefusal>>,
    interpretation: OnceLock<Result<(), InterpretationError>>,
    names: OnceLock<Result<NameIndex, InterpretationError>>,
    bindings: NamespaceCache,
    sources: NamespaceCache,
    dependencies: NamespaceCache,
}
impl fmt::Debug for ParsedDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedDocument")
            .field("value", &self.inner.value)
            .finish()
    }
}
#[derive(Clone, Debug)]
/// Normative evidence plus the parsed snapshot when one was admitted. Invalid JSON can still produce a report without a parsed document.
pub struct DocumentAssessment {
    document: Option<ParsedDocument>,
    report: Arc<ConformanceReport>,
}
#[derive(Clone, Debug)]
/// Retained proof that this immutable snapshot satisfied every applicable normative document rule. Obtain it through [`DocumentAssessment::validated`]; contract preparation is still a separate evaluator-dependent step.
pub struct ValidatedDocument {
    document: ParsedDocument,
}
#[derive(Clone, Debug, PartialEq, Eq)]
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

impl ParsedDocument {
    /// Parse and retain exact bytes and source locations without establishing conformance.
    /// Use [`Self::assess`] before treating this as a conformant document. See the
    /// [Rust first-use guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md)
    /// for the progression from parsing to a prepared input contract.
    pub fn parse(input: impl AsRef<[u8]>) -> Result<Self, InputError> {
        JsonValue::parse(input).map(Self::from_json)
    }
    /// Create a document snapshot from an exact value without assessment. A subtree is made standalone so subsequent document coordinates refer to its independent source.
    pub fn from_json(value: JsonValue) -> Self {
        let value = backend::standalone(value);
        Self {
            inner: Arc::new(DocumentInner {
                value,
                schemas: OnceLock::new(),
                assessment: OnceLock::new(),
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
    /// Borrow the complete immutable source bytes, including whitespace.
    pub fn original_bytes(&self) -> &[u8] {
        self.value().original_source()
    }
    /// Copy representable normative fields into an editable draft. Refuses duplicates and invalid typed shapes; exact opaque fields retain their values. This conversion does not establish conformance.
    pub fn to_authoring(&self) -> Result<DocumentBuilder, AuthoringError> {
        DocumentBuilder::from_json(self.value())
    }
    /// Assess every document rule, retaining findings at original source locations.
    /// A supported version can still be nonconformant or undetermined; only
    /// [`DocumentAssessment::validated`] yields a conformance proof. An unsupported
    /// declared version is a separate refusal. Explanatory finding messages are
    /// human-readable guidance; use rule/code/evidence fields for program logic.
    pub fn assess(&self) -> Result<DocumentAssessment, VersionRefusal> {
        let report = self
            .inner
            .assessment
            .get_or_init(|| assess_value(self).map(Arc::new))
            .clone()?;
        Ok(DocumentAssessment {
            document: Some(self.clone()),
            report,
        })
    }
    pub(crate) fn schemas(&self) -> &SchemaIndex {
        self.inner
            .schemas
            .get_or_init(|| SchemaIndex::build(self.value()))
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
        if backend::has_unpaired(self.value()) {
            return Err(InterpretationError::UnpairedString);
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
    pub fn operations(&self) -> Result<Vec<OperationView>, InterpretationError> {
        self.interpretable()?;
        self.names()?
            .operations
            .iter()
            .map(|(key, value)| self.operation_view(key, value))
            .collect()
    }
    /// Lexical presentation order; this does not select or rank bindings.
    pub fn operation_bindings(&self, key: &str) -> Result<Vec<String>, InterpretationError> {
        self.interpretable()?;
        let index = self.names()?;
        if !index.operations.contains_key(key) {
            return Ok(Vec::new());
        }
        Ok(index.bindings.get(key).cloned().unwrap_or_default())
    }
    /// Return `None` when the dependency is absent, otherwise test its kind filter. Absence of `kinds` accepts all; an empty list accepts none. Interpretation may refuse the document; this method is not normative proof.
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
                let Some(key) = member.name.as_str() else {
                    continue;
                };
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
                if let (Some(name), Some(operation)) = (
                    member.name.as_str(),
                    member.value.get("operation").and_then(|v| v.as_str()),
                ) {
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
#[derive(Clone, Debug)]
/// Name-resolution result after successful interpretation; missing and ambiguous names are not schema verdicts.
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
/// Immutable retained operation object. Its exact value is not conformance proof.
#[derive(Clone, Debug)]
pub struct OperationView {
    document: ParsedDocument,
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
    /// Use [`crate::ValueContracts::prepare`] to establish contract readiness.
    pub fn input(&self) -> Option<JsonRef<'_>> {
        self.value.get("input")
    }
    /// The exact output field, with the same absence/interpretation distinction
    /// as [`Self::input`]. A present value is not proof of a supported contract.
    pub fn output(&self) -> Option<JsonRef<'_>> {
        self.value.get("output")
    }
    /// Interpret an optional string description. Unlike exact [`Self::input`]
    /// access, this can refuse a malformed field with its original location.
    pub fn description(&self) -> Result<Option<&str>, InterpretationError> {
        self.value
            .get("description")
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| InterpretationError::invalid("invalid-operation-description", v))
            })
            .transpose()
    }
    /// Interpret optional string aliases, preserving absence separately from an
    /// empty array. A malformed array or item is a located interpretation error.
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
    pub fn bindings(&self) -> Result<Vec<String>, InterpretationError> {
        self.document.operation_bindings(&self.key)
    }
}
impl DocumentAssessment {
    /// Borrow complete per-rule evidence and the bounded findings list.
    pub fn report(&self) -> &ConformanceReport {
        &self.report
    }
    /// Borrow the admitted snapshot, or `None` when input could not be parsed within admission limits.
    pub fn parsed(&self) -> Option<&ParsedDocument> {
        self.document.as_ref()
    }
    /// Return a retained proof only for a conformant report; nonconformant and undetermined reports return `None`.
    pub fn validated(&self) -> Option<ValidatedDocument> {
        if self.report.conclusion == Conformance::Conformant {
            self.document
                .clone()
                .map(|document| ValidatedDocument { document })
        } else {
            None
        }
    }
}
impl ValidatedDocument {
    /// Borrow the exact snapshot certified by this proof, for interpretation or contract setup.
    pub fn parsed(&self) -> &ParsedDocument {
        &self.document
    }
    /// Borrow the exact bytes certified by this proof, including original formatting.
    pub fn original_bytes(&self) -> &[u8] {
        self.document.original_bytes()
    }
}
impl DocumentBuilder {
    /// Encode the current draft into a fresh independent parsed snapshot. May refuse collisions, unrepresentable fields or limits; does not establish conformance.
    pub fn build(&self) -> Result<ParsedDocument, AuthoringError> {
        self.to_json().map(ParsedDocument::from_json)
    }
}

/// Assess bytes even when parsing fails. A version refusal is never conformance evidence.
pub fn assess_document(input: impl AsRef<[u8]>) -> Result<DocumentAssessment, VersionRefusal> {
    let bytes = input.as_ref();
    // Flat storage permits a grammar scan past the public carriage-depth boundary,
    // while byte/node admission still bounds memory. This preserves version priority.
    match JsonValue::parse_with_limits(
        bytes,
        JsonLimits {
            max_depth: usize::MAX,
            ..Default::default()
        },
    ) {
        Ok(value) => ParsedDocument::from_json(value).assess(),
        Err(error) => {
            let mut checks = Checks::new();
            let limit = error.kind == InputErrorKind::Limit;
            if limit {
                for rule in 0..13 {
                    checks.mark(
                        rule,
                        Evidence::Inconclusive,
                        "input-limit",
                        None,
                        error.to_string(),
                    );
                }
            } else {
                let offset = error.byte_offset.min(bytes.len());
                let prefix = &bytes[..offset];
                let line = 1 + prefix.iter().filter(|&&b| b == b'\n').count();
                let byte_column = 1 + prefix.iter().rev().take_while(|&&b| b != b'\n').count();
                checks.mark(
                    0,
                    Evidence::Violated,
                    error.code,
                    Some(SourceLocation {
                        pointer: None,
                        byte_offset: offset,
                        line,
                        byte_column,
                    }),
                    error.to_string(),
                );
                checks.not_applicable_after_json();
            }
            Ok(DocumentAssessment {
                document: None,
                report: Arc::new(checks.finish()),
            })
        }
    }
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
    (check_version(declared) == VersionDecision::Unsupported).then(|| VersionRefusal {
        declared: declared.into(),
        supported: SUPPORTED_VERSIONS,
    })
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
    truncated: bool,
}
impl<'a> Checks<'a> {
    fn new() -> Self {
        Self {
            evidence: [Evidence::Satisfied; 13],
            findings: Vec::new(),
            pending_locations: Vec::new(),
            retained_at: HashMap::new(),
            retained_direct: Vec::new(),
            pointer_bytes: MAX_FINDING_POINTER_BYTES,
            truncated: false,
        }
    }
    fn update_evidence(&mut self, rule: usize, status: Evidence) {
        if status == Evidence::Violated || self.evidence[rule] != Evidence::Violated {
            self.evidence[rule] = status;
        }
    }
    fn same_finding(
        &self,
        index: usize,
        rule: usize,
        status: Evidence,
        code: &str,
        message: &str,
    ) -> bool {
        let finding = &self.findings[index];
        finding.rule == DOCUMENT_RULES[rule]
            && finding.status == status
            && finding.code == code
            && finding.message == message
    }
    fn contains_at(
        &self,
        rule: usize,
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
        rule: usize,
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
                rule: DOCUMENT_RULES[rule],
                status,
                code,
                location,
                message,
            });
        } else {
            self.truncated = true;
        }
    }
    fn mark_at(
        &mut self,
        rule: usize,
        status: Evidence,
        at: JsonRef<'a>,
        code: &'static str,
        message: impl Into<String>,
    ) {
        self.update_evidence(rule, status);
        let message = message.into();
        // Identity is checked before either allowance. Only retained findings
        // have index entries, so omitted data never grows an all-seen set.
        if self.contains_at(rule, status, at, code, &message) {
            return;
        }
        if self.findings.len() >= MAX_FINDINGS {
            self.truncated = true;
            return;
        }
        let Some(bytes) = backend::location_pointer_size(at, self.pointer_bytes) else {
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
            rule: DOCUMENT_RULES[rule],
            status,
            code,
            location: None,
            message,
        });
    }
    fn violation(
        &mut self,
        rule: usize,
        at: JsonRef<'a>,
        code: &'static str,
        message: impl Into<String>,
    ) {
        self.mark_at(rule, Evidence::Violated, at, code, message);
    }
    fn not_applicable_after_json(&mut self) {
        self.evidence[1..].fill(Evidence::NotApplicable);
    }
    fn finish(mut self) -> ConformanceReport {
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
        ConformanceReport {
            release: APPLIED_SPEC_RELEASE,
            revision: APPLIED_SPEC_REVISION,
            policy: "openbindings-rust/0.2.0-alpha.1",
            conclusion,
            evidence: DOCUMENT_RULES.into_iter().zip(self.evidence).collect(),
            findings: self.findings,
            findings_truncated: self.truncated,
        }
    }
    fn fixed(&mut self, value: &'a JsonValue, rule: usize, is_meta: bool) {
        match fixed_schema::check_with_retained(
            value,
            is_meta,
            MAX_FINDINGS.saturating_sub(self.findings.len()),
            |at, message| {
                self.contains_at(rule, Evidence::Violated, at, "schema-mismatch", message)
            },
        ) {
            Ok(problems) => {
                self.truncated |= problems.truncated;
                if problems.violated {
                    self.evidence[rule] = Evidence::Violated;
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
                value.view(),
                "fixed-schema-limit-or-failure",
                reason,
            ),
        }
    }
}
const NAME_GRAMMAR_MESSAGE: &str = "a name must be a nonempty ASCII string: start with a letter, digit or underscore; then use letters, digits, underscores, dots or hyphens";
/// Test the normative name grammar: ASCII letter, digit or underscore first, followed by ASCII letters, digits, underscores, dots or hyphens. No Unicode normalization or case folding is performed.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().enumerate().all(|(i, c)| {
            c.is_ascii_alphanumeric() || c == b'_' || (i > 0 && (c == b'-' || c == b'.'))
        })
}
fn assess_value(document: &ParsedDocument) -> Result<ConformanceReport, VersionRefusal> {
    let value = document.value();
    if let Some(refusal) = version_refusal(value) {
        return Err(refusal);
    }
    let mut c = Checks::new();
    let duplicates = backend::duplicate_member_names(value);
    if duplicates.len() != 0 {
        c.truncated = duplicates.len() > MAX_FINDINGS;
        for at in duplicates.take(MAX_FINDINGS) {
            c.mark_at(
                0,
                Evidence::Violated,
                at,
                "duplicate-member",
                "object repeats a decoded member name",
            );
        }
        c.not_applicable_after_json();
        return Ok(c.finish());
    }
    match value.get("openbindings") {
        Some(v)
            if v.as_str()
                .is_some_and(|s| check_version(s) != VersionDecision::Malformed) => {}
        Some(v) => c.violation(2, v, "semver", "openbindings must be a SemVer 2.0.0 string"),
        None => c.violation(
            2,
            value.view(),
            "missing-version",
            "required openbindings member is absent",
        ),
    }
    if backend::has_unpaired(value) || backend::depth(value) > 10_000 {
        for rule in 1..13 {
            if rule != 2 {
                c.mark_at(
                    rule,
                    Evidence::Inconclusive,
                    value.view(),
                    "representation-limit",
                    "interpretation requires supported strings and document nesting",
                );
            }
        }
        return Ok(c.finish());
    }
    c.fixed(value, 1, false);
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
                if !entry.name.as_str().is_some_and(valid_name) {
                    c.violation(3, entry.name, "name-grammar", NAME_GRAMMAR_MESSAGE);
                }
            }
        }
    }
    let mut names: HashMap<String, JsonValue> = HashMap::new();
    if let Some(entries) = operations.and_then(|v| v.members()) {
        let entries: Vec<_> = entries.collect();
        for entry in &entries {
            if let Some(name) = entry.name.as_str() {
                names.insert(name.into(), entry.name.to_owned());
            }
        }
        for entry in entries {
            if let Some(aliases) = entry.value.get("aliases").and_then(|v| v.elements()) {
                for alias in aliases {
                    let Some(name) = alias.as_str() else {
                        c.violation(3, alias, "name-grammar", NAME_GRAMMAR_MESSAGE);
                        continue;
                    };
                    if !valid_name(name) {
                        c.violation(3, alias, "name-grammar", NAME_GRAMMAR_MESSAGE);
                    }
                    if names.insert(name.into(), alias.to_owned()).is_some() {
                        c.violation(
                            4,
                            alias,
                            "duplicate-operation-name",
                            "operation keys and aliases must be distinct",
                        );
                    }
                }
            }
            if let Some(examples) = entry.value.get("examples").and_then(|v| v.members()) {
                for example in examples {
                    if !example.name.as_str().is_some_and(valid_name) {
                        c.violation(3, example.name, "name-grammar", NAME_GRAMMAR_MESSAGE);
                    }
                }
            }
        }
    }
    for (map, field, targets, rule) in [
        ("bindings", "operation", operations, 5),
        ("bindings", "source", sources, 6),
        ("dependencies", "operation", operations, 7),
    ] {
        if let Some(entries) = value.get(map).and_then(|v| v.members()) {
            for entry in entries {
                if let Some(reference) = entry.value.get(field)
                    && reference
                        .as_str()
                        .and_then(|name| targets.and_then(|v| v.get(name)))
                        .is_none()
                {
                    c.violation(
                        rule,
                        reference,
                        "missing-target",
                        "reference must name an existing primary map key",
                    );
                }
            }
        }
    }
    let schemas = document.schemas();
    // The index visits each contained schema occurrence once. Equal text at
    // different positions must still be checked at each original location.
    for node in &schemas.nodes {
        let schema = node.value.view();
        if node.depth > 256 {
            c.mark_at(
                9,
                Evidence::Inconclusive,
                schema,
                "schema-depth-limit",
                "schema depth exceeds 256",
            );
        } else {
            c.fixed(&node.value, 9, true);
        }
        if let Some(dialect) = schema.get("$schema")
            && !matches!(
                dialect.as_str(),
                Some(DIALECT) | Some("https://json-schema.org/draft/2020-12/schema#")
            )
        {
            c.violation(
                8,
                dialect,
                "schema-dialect",
                "contained schemas must name the 2020-12 dialect",
            );
        }
        if node.obi_position {
            if let Some(id) = schema.get("$id") {
                if !id.as_str().is_some_and(uri::absolute) {
                    c.violation(
                        10,
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
                            10,
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
                            10,
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
                            11,
                            reference,
                            "reference-target",
                            "same-document reference identifies no schema at an OBI position",
                        );
                    }
                }
            }
        }
    }
    for ((resource, _), declarations) in &schemas.anchors {
        if *resource == 0 && declarations.len() > 1 {
            for (value, keyword) in declarations {
                c.violation(
                    12,
                    value.get(keyword).unwrap(),
                    "duplicate-anchor",
                    "plain name is declared more than once in the document resource",
                );
            }
        }
    }
    for declarations in schemas.identifiers.values() {
        if declarations.len() > 1 {
            for value in declarations {
                c.violation(
                    12,
                    value.get("$id").unwrap(),
                    "duplicate-schema-id",
                    "schemas declare the same compared identifier",
                );
            }
        }
    }
    Ok(c.finish())
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
            let document = ParsedDocument::parse(source.to_string()).unwrap();
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
        let document = ParsedDocument::parse(source.to_string()).unwrap();
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
    fn final_dedup_precedes_count_and_pointer_caps_and_preserves_evidence() {
        let value = JsonValue::parse(r#"{"a":0,"b":0}"#).unwrap();
        let a = value.get("a").unwrap();
        let b = value.get("b").unwrap();
        let mut checks = Checks::new();
        checks.pointer_bytes = 2;
        checks.violation(0, a, "test", "first");
        checks.violation(0, a, "test", "first");
        assert_eq!(checks.pointer_bytes, 0);
        assert_eq!(checks.findings.len(), 1);
        assert!(!checks.truncated);
        checks.violation(1, b, "test", "distinct");
        assert!(checks.truncated);
        assert_eq!(checks.retained_at.len(), 1);
        assert_eq!(checks.finish().evidence["OBI-02"], Evidence::Violated);

        let mut checks = Checks::new();
        for i in 0..MAX_FINDINGS {
            checks.violation(0, a, "test", format!("message {i}"));
        }
        checks.violation(0, a, "test", "message 0");
        assert!(!checks.truncated);
        assert_eq!(
            checks.retained_at.values().map(Vec::len).sum::<usize>(),
            MAX_FINDINGS
        );
        checks.violation(1, a, "test", "message 0");
        assert!(checks.truncated);
        assert_eq!(checks.evidence[1], Evidence::Violated);
    }

    #[test]
    fn final_identity_includes_rule_status_code_message_and_original_occurrence() {
        let value = JsonValue::parse(r#"{"k":0,"k":0,"k":0}"#).unwrap();
        let nodes: Vec<_> = backend::duplicate_member_names(&value).collect();
        let mut checks = Checks::new();
        for node in &nodes {
            checks.violation(0, *node, "test", "same");
        }
        checks.violation(1, nodes[0], "test", "same");
        checks.mark_at(0, Evidence::Inconclusive, nodes[0], "test", "same");
        checks.violation(0, nodes[0], "other", "same");
        checks.violation(0, nodes[0], "test", "other");
        checks.violation(0, nodes[0], "test", "same");
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
    fn fixed_stream_excludes_already_retained_findings_before_intermediate_cap() {
        let value = JsonValue::parse("null").unwrap();
        let mut checks = Checks::new();
        checks.fixed(&value, 2, true);
        let fixed_count = checks.findings.len();
        for i in fixed_count..MAX_FINDINGS {
            checks.mark(
                0,
                Evidence::Violated,
                "padding",
                None,
                format!("distinct {i}"),
            );
        }
        checks.fixed(&value, 2, true);
        assert_eq!(checks.findings.len(), MAX_FINDINGS);
        assert!(!checks.truncated);
        assert_eq!(checks.evidence[2], Evidence::Violated);
    }
}
