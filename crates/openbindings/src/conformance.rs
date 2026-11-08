//! Document conformance: rule evidence, findings, admission and proof creation.
use crate::{
    fixed_schema,
    schema::inventory::{self, DIALECT, SameDocument},
    *,
};
use openbindings_internal_json::backend;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    sync::Arc,
};

/// Stable rule identifiers OBI-01 through OBI-14, in specification order.
pub const DOCUMENT_RULES: [&str; 14] = [
    "OBI-01", "OBI-02", "OBI-03", "OBI-04", "OBI-05", "OBI-06", "OBI-07", "OBI-08", "OBI-09",
    "OBI-10", "OBI-11", "OBI-12", "OBI-13", "OBI-14",
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
    Obi14,
}
impl Rule {
    const ALL: [Rule; 14] = [
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
        Rule::Obi14,
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
/// One explanatory normative-rule finding. OBI-02 carries the failed fixed-schema
/// keyword, its published schema origin and optional [`ValueProblemDetails`].
/// Core still checks only document conformance, never example or caller values.
/// Findings may be bounded; the report's independent rule evidence determines its conclusion. Serializes pointer fields as ordinary JSON strings, replacing unpaired UTF-16 units with U+FFFD; the stored [`Pointer`] stays exact for locating and editing.
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
    /// Original-source coordinates, absent only for a finding about no particular position, without a parser offset. Admission limits carry the parser offset and no pointer. Pointers are exact, are data and must be escaped for display; byte columns are not UTF-16 editor columns.
    pub location: Option<SourceLocation>,
    /// Human-facing explanation; display as text and use `rule`, `code` and `status` for logic.
    pub message: String,
    /// Failed OBI-02 keyword in the published fixed schema. Absent for other rules
    /// or when keyword guidance is unavailable. Failed unions name the applicator.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyword: Option<&'static str>,
    /// Keyword location in the published fixed schema, identified by its URI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaOrigin>,
    /// Complete schema facts using the value evaluator's vocabulary. Required
    /// emits one finding per missing member. Disallowed members are identified
    /// only by `location`; anyOf/oneOf, pattern and uniqueItems have no detail.
    /// Never copies rejected document values. The keyword/schema/details tuple
    /// has a measured maximum of 199 serialized JSON bytes in the pinned 0.2
    /// schema, guarded by an exhaustive schema test rather than a runtime cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ValueProblemDetails>,
}
impl std::fmt::Debug for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rule = if DOCUMENT_RULES.contains(&self.rule) {
            self.rule
        } else {
            "<redacted>"
        };
        // Fixed Checks emissions and parser codes forwarded by admission.
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
            | "byte-limit"
            | "depth-limit"
            | "node-limit"
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
            | "duplicate-schema-id"
            | "document-dynamic-anchor" => self.code,
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
/// Assessment of all normative document rules at the pinned specification revision. At most 4096 findings and 8 MiB of aggregate generated-pointer bytes (UTF-8, or two per UTF-16 unit for a pointer through a name with an unpaired surrogate) are retained. Omitted findings set `findings_truncated`; rule evidence and the conclusion remain independent of presentation caps. Retained coordinates always refer to original source. Serializes pointer fields as ordinary JSON strings, replacing unpaired UTF-16 units with U+FFFD; the stored [`Pointer`] stays exact for locating and editing.
///
/// The pinned 0.2 schema's largest OBI-02 keyword/schema/details tuple is 199
/// serialized JSON bytes. An exhaustive schema test guards this fixed property;
/// there is no runtime fact-size cap. Facts are never partial.
/// Failed anyOf/oneOf report the applicator with no branch detail. Missing required
/// members produce separate findings in the published array order. Other keywords
/// at one source position use fixed keyword order; source traversal is unchanged.
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
    pub(crate) document: Option<Document>,
    pub(crate) report: Arc<ValidationReport>,
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
    pub fn conformant(&self) -> Option<ConformantDocument> {
        if self.report.conclusion == Conformance::Conformant {
            self.document
                .clone()
                .map(|document| ConformantDocument { document })
        } else {
            None
        }
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
/// Limit findings use `byte-limit`, `depth-limit` or `node-limit`, with the parser's
/// byte offset and no pointer. Their messages describe the limit without API advice.
/// To admit more, call this function with a larger [`JsonLimits::max_bytes`],
/// [`JsonLimits::max_depth`] or [`JsonLimits::max_nodes`], respectively.
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
    if error.kind() == InputErrorKind::Limit {
        let message = input_limit_message(error.code(), limits, bytes.len());
        let location = backend::input_location(bytes, error.byte_offset());
        for rule in Rule::ALL {
            checks.mark(
                rule,
                Evidence::Inconclusive,
                error.code(),
                Some(location.clone()),
                message.as_str(),
            );
        }
    } else {
        checks.mark(
            Rule::Obi01,
            Evidence::Violated,
            error.code(),
            Some(backend::input_location(bytes, error.byte_offset())),
            parse_error_message(error.code()),
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
pub(crate) fn version_refusal(value: &JsonValue) -> Option<VersionRefusal> {
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
// Human-facing admission facts; recovery API advice belongs in API docs.
fn input_limit_message(code: &str, limits: JsonLimits, bytes: usize) -> String {
    match code {
        "byte-limit" => format!(
            "the document's {bytes} bytes exceed the {}-byte admission limit",
            limits.max_bytes
        ),
        "node-limit" => format!(
            "the document exceeds the admission limit of {} JSON nodes",
            limits.max_nodes.min(u32::MAX as usize)
        ),
        _ => format!(
            "the document nests containers deeper than the {} levels admitted",
            limits.max_depth
        ),
    }
}
fn fixed_details(keyword: &str, operand: &serde_json::Value) -> Option<ValueProblemDetails> {
    use ValueProblemDetails as D;
    match keyword {
        "required" => Some(D::Required {
            member: operand.as_str()?.to_owned(),
        }),
        "type" => Some(D::Type {
            expected: match operand {
                serde_json::Value::String(name) => vec![name.clone()],
                serde_json::Value::Array(names) => names
                    .iter()
                    .map(|n| n.as_str().map(str::to_owned))
                    .collect::<Option<_>>()?,
                _ => return None,
            },
        }),
        "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" => Some(D::NumericBound {
            bound: operand.to_string(),
        }),
        "minLength" | "maxLength" | "minItems" | "maxItems" => Some(D::SizeBound {
            bound: operand.to_string(),
        }),
        "enum" => Some(D::Enum {
            choices: operand
                .as_array()?
                .iter()
                .map(ToString::to_string)
                .collect(),
        }),
        _ => None,
    }
}
struct FixedFact {
    keyword: &'static str,
    schema: SchemaOrigin,
    details: Option<ValueProblemDetails>,
}
impl FixedFact {
    fn new(keyword: &'static str, pointer: &str, operand: &serde_json::Value) -> Self {
        Self {
            keyword,
            schema: SchemaOrigin::new(
                Some("https://openbindings.com/schema/openbindings-0.2.0.json".into()),
                pointer.parse().expect("fixed schema pointer"),
            ),
            details: fixed_details(keyword, operand),
        }
    }
}
fn fixed_facts() -> &'static [FixedFact] {
    static FACTS: std::sync::OnceLock<Vec<FixedFact>> = std::sync::OnceLock::new();
    FACTS.get_or_init(|| {
        fixed_schema::fact_templates()
            .iter()
            .map(|(keyword, pointer, operand)| FixedFact::new(keyword, pointer, operand))
            .collect()
    })
}
const MAX_FINDINGS: usize = 4096;
const MAX_FINDING_POINTER_BYTES: usize = backend::MAX_REPORT_POINTER_BYTES;
struct Checks<'a> {
    evidence: [Evidence; 14],
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
            evidence: [Evidence::Met; 14],
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
                keyword: None,
                schema: None,
                details: None,
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
        self.mark_at_prepared_fact(rule, status, at, code, message, None);
    }
    #[cfg(test)]
    fn mark_at_fact<M: AsRef<str> + Into<String>>(
        &mut self,
        rule: Rule,
        status: Evidence,
        at: JsonRef<'a>,
        code: &'static str,
        message: M,
        fact: Option<(&'static str, String, serde_json::Value)>,
    ) {
        let fact =
            fact.map(|(keyword, pointer, operand)| FixedFact::new(keyword, &pointer, &operand));
        self.mark_at_prepared_fact(rule, status, at, code, message, fact.as_ref());
    }
    fn mark_at_prepared_fact<M: AsRef<str> + Into<String>>(
        &mut self,
        rule: Rule,
        status: Evidence,
        at: JsonRef<'a>,
        code: &'static str,
        message: M,
        fact: Option<&FixedFact>,
    ) {
        self.update_evidence(rule, status);
        // Identity is checked before either allowance. Only retained findings
        // have index entries, so omitted data never grows an all-seen set.
        if self
            .retained_at
            .get(&fixed_schema::occurrence(at))
            .is_some_and(|indices| {
                indices.iter().any(|&index| {
                    let finding = &self.findings[index];
                    self.same_finding(index, rule, status, code, message.as_ref())
                        && finding.keyword == fact.map(|f| f.keyword)
                        && finding.schema.as_ref() == fact.map(|f| &f.schema)
                        && finding.details.as_ref() == fact.and_then(|f| f.details.as_ref())
                })
            })
        {
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
        // Only the public report owns copies. Intermediate problems carry a
        // plan index, and duplicate or omitted findings allocate no fact data.
        let (keyword, schema, details) = match fact {
            Some(fact) => (
                Some(fact.keyword),
                Some(fact.schema.clone()),
                fact.details.clone(),
            ),
            None => (None, None, None),
        };
        self.findings.push(Finding {
            rule: rule.id(),
            status,
            code,
            location: None,
            message: message.into(),
            keyword,
            schema,
            details,
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
    fn meta_fixed(&mut self, meta: &mut inventory::MetaChecks<'a>) {
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
                    self.mark_at_prepared_fact(
                        rule,
                        Evidence::Violated,
                        problem.at,
                        "schema-mismatch",
                        problem.message,
                        problem.fact.map(|index| &fixed_facts()[index]),
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
pub(crate) fn assess_value(document: &Document) -> Result<ValidationReport, VersionRefusal> {
    let value = document.value();
    if let Some(refusal) = version_refusal(value) {
        return Err(refusal);
    }
    let mut c = Checks::new();
    // Version declaration follows §8.1's JSON grammar, independently of this
    // line's OBI-01 exclusions. OBI-02 through OBI-14 require OBI-01 (§10).
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
        if node.resource == 0
            && let Some(anchor) = schema.get("$dynamicAnchor")
        {
            c.violation(
                Rule::Obi14,
                anchor,
                "document-dynamic-anchor",
                "a dynamic anchor must belong to an identified schema resource",
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
                        && matches!(
                            schemas.same_document(value, text),
                            SameDocument::Missing | SameDocument::Ambiguous
                        )
                    {
                        c.violation(
                            Rule::Obi12,
                            reference,
                            "reference-target",
                            "same-document reference identifies no contained schema",
                        );
                    }
                }
            }
        }
    }
    meta.finish();
    for reference in schemas.missing_contained_targets() {
        c.violation(
            Rule::Obi12,
            reference,
            "reference-target",
            "reference into a contained resource identifies no contained schema",
        );
    }
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

#[cfg(test)]
mod finding_identity_tests {
    use super::*;
    #[test]
    fn pinned_schema_bounds_every_public_finding_fact() {
        let plan = fixed_schema::compile_locations().unwrap();
        let value = JsonValue::parse("{}").unwrap();
        let mut measured = Vec::new();
        plan.visit_possible_facts(|keyword, pointer, operand| {
            // Exercise the exact public conversion, including JSON escaping,
            // schema origin and absent details, without selecting a document.
            let mut checks = Checks::new();
            checks.mark_at_fact(
                Rule::Obi02,
                Evidence::Violated,
                value.view(),
                "schema-mismatch",
                "value violates the fixed normative schema",
                Some((keyword, pointer.to_owned(), operand.clone())),
            );
            let report = checks.finish();
            assert!(!report.findings_truncated);
            assert_eq!(report.evidence["OBI-02"], Evidence::Violated);
            assert_eq!(report.conclusion, Conformance::NonConformant);
            let [finding] = report.findings.as_slice() else {
                panic!("every schema fact must retain one complete finding");
            };
            let size = serde_json::to_vec(&(&finding.keyword, &finding.schema, &finding.details))
                .unwrap()
                .len();
            measured.push((size, keyword, pointer.to_owned()));
        });
        measured.sort();
        // Includes 61 Assertions (one anyOf) and all six forbidden shapes.
        // A schema update must requalify this property, never truncate a fact.
        assert_eq!(measured.len(), 67);
        assert_eq!(
            measured.last().unwrap(),
            &(
                199,
                "minimum",
                "/$defs/BindingEntry/properties/preference/minimum".into()
            )
        );
    }

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
