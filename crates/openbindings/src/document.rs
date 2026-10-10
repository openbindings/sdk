//! Immutable document snapshots and complete conformance evidence.
use crate::{
    fixed_schema,
    schema_index::{DIALECT, SameDocument, SchemaIndex},
    *,
};
use openbindings_internal_json::backend;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    sync::{Arc, OnceLock},
};

pub const DOCUMENT_RULES: [&str; 13] = [
    "OBI-01", "OBI-02", "OBI-03", "OBI-04", "OBI-05", "OBI-06", "OBI-07", "OBI-08", "OBI-09",
    "OBI-10", "OBI-11", "OBI-12", "OBI-13",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Evidence {
    Satisfied,
    Violated,
    Inconclusive,
    NotApplicable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Conformance {
    Conformant,
    NonConformant,
    Undetermined,
}
#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub rule: &'static str,
    pub status: Evidence,
    pub code: &'static str,
    pub location: Option<SourceLocation>,
    pub message: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct ConformanceReport {
    pub release: &'static str,
    pub revision: &'static str,
    pub policy: &'static str,
    pub conclusion: Conformance,
    pub evidence: BTreeMap<&'static str, Evidence>,
    pub findings: Vec<Finding>,
    pub findings_truncated: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VersionRefusal {
    pub declared: String,
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
pub struct ParsedDocument {
    pub(crate) inner: Arc<DocumentInner>,
}
pub(crate) struct DocumentInner {
    pub value: JsonValue,
    pub schemas: OnceLock<SchemaIndex>,
    assessment: OnceLock<Result<Arc<ConformanceReport>, VersionRefusal>>,
    interpretation: OnceLock<Result<(), InterpretationError>>,
    names: OnceLock<Result<NameIndex, InterpretationError>>,
}
impl fmt::Debug for ParsedDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedDocument")
            .field("value", &self.inner.value)
            .finish()
    }
}
#[derive(Clone, Debug)]
pub struct DocumentAssessment {
    document: Option<ParsedDocument>,
    report: Arc<ConformanceReport>,
}
#[derive(Clone, Debug)]
pub struct ValidatedDocument {
    document: ParsedDocument,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InterpretationError {
    Version(VersionRefusal),
    MalformedVersion,
    DuplicateMembers,
    UnpairedString,
    InvalidField {
        code: &'static str,
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
    pub fn code(&self) -> &'static str {
        match self {
            Self::Version(_) => "unsupported-version",
            Self::MalformedVersion => "malformed-version",
            Self::DuplicateMembers => "duplicate-members",
            Self::UnpairedString => "unpaired-string",
            Self::InvalidField { code, .. } => code,
        }
    }
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
    pub fn from_json(value: JsonValue) -> Self {
        let value = backend::standalone(value);
        Self {
            inner: Arc::new(DocumentInner {
                value,
                schemas: OnceLock::new(),
                assessment: OnceLock::new(),
                interpretation: OnceLock::new(),
                names: OnceLock::new(),
            }),
        }
    }
    pub fn value(&self) -> &JsonValue {
        &self.inner.value
    }
    pub fn original_bytes(&self) -> &[u8] {
        self.value().original_source()
    }
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
    fn names(&self) -> Result<&NameIndex, InterpretationError> {
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
            let candidates = matches
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            return Ok(OperationSelection::Ambiguous { candidates });
        }
        let key = &matches[0];
        self.operation_view(key, &index.operations[key])
            .map(OperationSelection::Found)
    }
    fn operation_view(
        &self,
        key: &str,
        value: &JsonValue,
    ) -> Result<OperationView, InterpretationError> {
        if value.kind() != JsonKind::Object {
            return Err(InterpretationError::invalid(
                "invalid-operation-object",
                value.view(),
            ));
        }
        Ok(OperationView {
            document: self.clone(),
            key: key.into(),
            value: value.clone(),
        })
    }
    /// Primary operation objects in lexical key order. Malformed entries refuse
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
    pub fn dependency_accepts_kind(
        &self,
        dependency: &str,
        kind: &str,
    ) -> Result<Option<bool>, InterpretationError> {
        self.interpretable()?;
        let Some(value) = self
            .value()
            .get("dependencies")
            .and_then(|v| v.get(dependency))
        else {
            return Ok(None);
        };
        Ok(Some(match value.get("kinds") {
            None => true,
            Some(kinds) => kinds
                .elements()
                .is_some_and(|mut a| a.any(|v| v.as_str() == Some(kind))),
        }))
    }
}
#[derive(Default)]
struct NameIndex {
    operations: BTreeMap<String, JsonValue>,
    names: HashMap<String, Vec<String>>,
    bindings: HashMap<String, Vec<String>>,
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
                index.operations.insert(key.into(), member.value.to_owned());
                index.add_name(key, key);
                if let Some(aliases) = member.value.get("aliases") {
                    let values = aliases.elements().ok_or_else(|| {
                        InterpretationError::invalid("invalid-operation-aliases", aliases)
                    })?;
                    for alias in values {
                        let name = alias.as_str().ok_or_else(|| {
                            InterpretationError::invalid("invalid-operation-alias", alias)
                        })?;
                        index.add_name(name, key);
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
    fn add_name(&mut self, name: &str, key: &str) {
        self.names.entry(name.into()).or_default().push(key.into());
    }
}
#[derive(Clone, Debug)]
pub enum OperationSelection {
    Found(OperationView),
    Missing,
    Ambiguous { candidates: Vec<String> },
}
/// Immutable retained operation object. Its exact value is not conformance proof.
#[derive(Clone, Debug)]
pub struct OperationView {
    document: ParsedDocument,
    key: String,
    value: JsonValue,
}
impl OperationView {
    pub fn key(&self) -> &str {
        &self.key
    }
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
    pub fn bindings(&self) -> Result<Vec<String>, InterpretationError> {
        self.document.operation_bindings(&self.key)
    }
}
impl DocumentAssessment {
    pub fn report(&self) -> &ConformanceReport {
        &self.report
    }
    pub fn parsed(&self) -> Option<&ParsedDocument> {
        self.document.as_ref()
    }
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
    pub fn parsed(&self) -> &ParsedDocument {
        &self.document
    }
    pub fn original_bytes(&self) -> &[u8] {
        self.document.original_bytes()
    }
}
impl DocumentBuilder {
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
    pointer_bytes: usize,
    truncated: bool,
}
impl<'a> Checks<'a> {
    fn new() -> Self {
        Self {
            evidence: [Evidence::Satisfied; 13],
            findings: Vec::new(),
            pending_locations: Vec::new(),
            pointer_bytes: MAX_FINDING_POINTER_BYTES,
            truncated: false,
        }
    }
    fn mark(
        &mut self,
        rule: usize,
        status: Evidence,
        code: &'static str,
        location: Option<SourceLocation>,
        message: impl Into<String>,
    ) {
        if status == Evidence::Violated || self.evidence[rule] != Evidence::Violated {
            self.evidence[rule] = status;
        }
        if self.findings.len() < MAX_FINDINGS {
            self.findings.push(Finding {
                rule: DOCUMENT_RULES[rule],
                status,
                code,
                location,
                message: message.into(),
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
        // Rule evidence is independent of retained diagnostic capacity. Keep
        // borrowed nodes until finish, then scan each source prefix only once.
        if status == Evidence::Violated || self.evidence[rule] != Evidence::Violated {
            self.evidence[rule] = status;
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
        self.pending_locations.push((self.findings.len(), at));
        self.mark(rule, status, code, None, message);
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
        match fixed_schema::check(
            value,
            is_meta,
            MAX_FINDINGS.saturating_sub(self.findings.len()),
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
    let duplicates = backend::duplicate_nodes(value);
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
    let mut meta_seen = HashSet::new();
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
        } else if meta_seen.insert(node.value.text()) {
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
