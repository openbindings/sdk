//! Immutable exact document snapshots and retained conformance evidence.
use crate::{
    conformance::{assess_value, version_refusal},
    schema::inventory::SchemaIndex,
    *,
};
use openbindings_internal_json::backend;
mod edit;
mod read_views;
pub use edit::{Edit, EditError, PointerRef};
mod conformant_views;
use crate::{schema::references::SchemaSpace, schema::resources::ExternalSchemasInner};
pub use conformant_views::*;
use read_views::NamespaceCache;
pub use read_views::{BindingView, DependencyView, ExampleView, SourceView};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    sync::{Arc, Mutex, OnceLock, PoisonError, Weak},
};

#[derive(Clone)]
/// Immutable exact OBI document without a conformance claim. Edits return a new snapshot. Clones share storage and lazily cached interpretation/validation. Dropping all owners releases storage; it does not promise an immediate process RSS decrease.
#[must_use = "documents are immutable; retain the returned snapshot"]
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
    spaces_in: Mutex<Vec<Weak<ExternalSchemasInner>>>,
    validation: OnceLock<Result<Arc<ValidationReport>, VersionRefusal>>,
    interpretation: OnceLock<Result<(), InterpretationError>>,
    names: OnceLock<Result<NameIndex, InterpretationError>>,
    bindings: NamespaceCache,
    sources: NamespaceCache,
    dependencies: NamespaceCache,
}
impl DocumentInner {
    /// Stop tracking a set that is being dropped.
    pub(crate) fn forget_set(&self, set: *const ExternalSchemasInner) {
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
/// fn forge(document: Document) { let _ = ConformantDocument { document }; }
/// ```
/// A proof exposes only its validated snapshot; editing produces a new document:
/// ```compile_fail
/// use openbindings::{ConformantDocument, Document};
/// fn edit(mut proof: ConformantDocument) { proof.set_description("changed").unwrap(); }
/// ```
/// ```compile_fail
/// use openbindings::{ConformantDocument, Document};
/// fn consume(proof: ConformantDocument) { let document = proof.into_inner(); proof.operations(); }
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
pub struct ConformantDocument {
    pub(crate) document: Document,
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
    pub(crate) document: Document,
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
impl ConformantDocument {
    /// The resolution space layering `resources` over this revision. It is
    /// assembled once per live pair: the space without resources lives with the
    /// revision, and a space with a nonempty set lives with that set until
    /// either the set or the revision is dropped. Alternating live sets reuses
    /// each set's space; nothing retains a space beyond its pair.
    pub(crate) fn resolution_bounded(
        &self,
        resources: &ExternalSchemas,
        maximum: usize,
        maximum_identity: usize,
        control: &dyn WorkControl,
    ) -> Result<Arc<SchemaSpace>, openbindings_internal_schema::Failure> {
        let inner = &self.document.inner;
        let cached = if resources.is_empty() {
            inner.resolution.get().cloned()
        } else {
            resources.kept_space(inner)
        };
        if let Some(space) = cached {
            space.check_inventory_limits(maximum, maximum_identity, control)?;
            return Ok(space);
        }
        // Failed or cancelled assembly never poisons either cache. Work and
        // allocation happen outside locks; a concurrent complete space may win.
        let built = Arc::new(SchemaSpace::new_bounded(
            &self.document,
            resources,
            maximum,
            maximum_identity,
            control,
        )?);
        if resources.is_empty() {
            let _ = inner.resolution.set(built);
            return Ok(inner.resolution.get().expect("completed space").clone());
        }
        let space = resources.keep_space(inner, built);
        let mut sets = inner
            .spaces_in
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        sets.retain(|set| set.strong_count() > 0);
        let set = resources.downgrade();
        if !sets.iter().any(|known| known.ptr_eq(&set)) {
            sets.push(set);
        }
        Ok(space)
    }
    /// Borrow the exact snapshot certified by this proof, for plain fallible reads.
    pub fn document(&self) -> &Document {
        &self.document
    }
    /// Consume the proof to retain its immutable document snapshot.
    pub fn into_inner(self) -> Document {
        self.document
    }
    /// Borrow the exact bytes certified by this proof, including original formatting.
    pub fn as_bytes(&self) -> &[u8] {
        self.document.as_bytes()
    }
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
mod resolution_reuse {
    use super::*;
    use crate::schema::references::ASSEMBLED;
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
    fn proof() -> ConformantDocument {
        Document::parse(
            r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/item"},"output":true}}}"#,
        )
        .unwrap()
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
    }
    fn snapshot(minimum: u32) -> ExternalSchemas {
        ExternalSchemas::new([ExternalSchema {
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
    fn kept(proof: &ConformantDocument, set: &ExternalSchemas) -> Option<Weak<SchemaSpace>> {
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
                    .prepare_with(&Projecting, &ExternalSchemas::default())
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
        assert!(b.upgrade().is_some()); // The contextual schema is still an owner.
        drop(input);
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
