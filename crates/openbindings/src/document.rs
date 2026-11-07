//! Immutable exact document snapshots and retained conformance evidence.
use crate::{conformance::assess_value, schema::inventory::SchemaIndex, *};
use openbindings_internal_json::backend;
mod edit;
mod index;
mod model;
use crate::{schema::references::SchemaSpace, schema::resources::ExternalSchemasInner};
pub use edit::{Edit, EditError, PointerRef};
use index::NamespaceCache;
pub use model::{Binding, Dependency, Example, Operation, Source};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
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
    // Original schema environments are assembled on demand. The resolution
    // space without supplied resources lives here;
    // a space with a nonempty set lives in that set, which this revision tells
    // to release it when the revision is dropped.
    resolution: OnceLock<Arc<SchemaSpace>>,
    spaces_in: Mutex<Vec<Weak<ExternalSchemasInner>>>,
    validation: OnceLock<Result<Arc<ValidationReport>, VersionRefusal>>,
    operations: OnceLock<index::Operations>,
    bindings: OnceLock<index::Bindings>,
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
/// Its canonical [`Operation`], [`Binding`], [`Source`], [`Dependency`] and
/// [`Example`] values retain this proof and their original entries. Field reads
/// and declared relationships are infallible; no detached authoring model or
/// partially interpreted draft is needed.
///
/// Proof has no public unchecked constructor or deserializer:
/// ```compile_fail
/// use openbindings::*;
/// fn forge(document: Document) { let _ = ConformantDocument { document }; }
/// ```
/// A proof exposes only its validated snapshot; editing produces a new document:
/// ```compile_fail
/// use openbindings::{ConformantDocument, Document};
/// fn edit(proof: ConformantDocument) {
///     let _: ConformantDocument = proof.document().edit(&[]).unwrap();
/// }
/// ```
/// ```compile_fail
/// use openbindings::{ConformantDocument, Document};
/// fn consume(proof: ConformantDocument) { let document = proof.into_inner(); proof.operations(); }
/// ```
/// Only a conformant operation hands out its input and output schemas:
/// ```compile_fail,E0599
/// use openbindings::Document;
/// fn schemas(doc: Document) { let _ = doc.operation("op"); }
/// ```
/// Obtain it through [`DocumentValidation::conformant`]; schema preparation is still a separate evaluator-dependent step.
pub struct ConformantDocument {
    pub(crate) document: Document,
}

impl Document {
    /// Parse and retain exact bytes and source locations without establishing conformance.
    /// Use [`Self::validate`] before treating this as a conformant document. See the
    /// [Rust first-use guide](https://github.com/openbindings/sdk/blob/main/docs/rust-first-use.md)
    /// for the progression from parsing to a prepared input schema.
    ///
    /// # Errors
    /// Returns an input error for invalid UTF-8, a byte order mark, invalid JSON syntax, or an exceeded admission limit.
    pub fn parse(input: impl AsRef<[u8]>) -> Result<Self, InputError> {
        Self::parse_with_limits(input, JsonLimits::default())
    }
    /// Parse under caller-selected byte, node and nesting admission limits, which
    /// the document keeps: its edits parse the edited document
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
                operations: OnceLock::new(),
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
    /// The admission limits this document was admitted under, which its edits apply: [`JsonLimits::default`] for [`Self::parse`],
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
        control: &dyn openbindings_internal_schema::Control,
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
    /// Borrow the exact snapshot certified by this proof, for raw inspection and editing.
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
#[cfg(test)]
mod resolution_reuse {
    use super::*;
    use crate::schema::references::ASSEMBLED;
    use std::sync::Weak;
    /// Retains the original environment while the prepared validator lives.
    struct Inspecting;
    struct Retaining(#[allow(dead_code)] SchemaEnvironment);
    impl SchemaEvaluator for Inspecting {
        type Validator = Retaining;
        fn prepare(
            &self,
            request: EvaluationRequest<'_>,
            limits: &PreparationLimits,
            control: &dyn WorkControl,
        ) -> Result<Retaining, PreparationFailure> {
            Ok(Retaining(request.environment(limits, control)?))
        }
    }
    /// An evaluator can read the selected schema without needing an inventory.
    struct Accepting;
    struct Snapshots {
        _schema: Schema,
        _external: ExternalSchemas,
    }
    impl SchemaEvaluator for Accepting {
        type Validator = Snapshots;
        fn prepare(
            &self,
            request: EvaluationRequest<'_>,
            _: &PreparationLimits,
            _: &dyn WorkControl,
        ) -> Result<Snapshots, PreparationFailure> {
            Ok(Snapshots {
                _schema: request.schema.clone(),
                _external: request.external_schemas.clone(),
            })
        }
    }
    impl SchemaValidator for Retaining {
        fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
            ValueOutcome::Satisfies
        }
    }
    impl SchemaValidator for Snapshots {
        fn evaluate(&self, _: &JsonValue, _: Detail, _: &dyn WorkControl) -> ValueOutcome {
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
    fn evaluators_that_never_inventory_assemble_no_space() {
        let proof = proof();
        let operation = proof.resolve_operation("op").unwrap();
        let set = snapshot(1);
        let before = assembled();
        drop(
            operation
                .input()
                .unwrap()
                .prepare(&Accepting, &set, &crate::PreparationLimits::default())
                .unwrap(),
        );
        drop(
            operation
                .output()
                .unwrap()
                .prepare(
                    &Accepting,
                    &crate::ExternalSchemas::default(),
                    &crate::PreparationLimits::default(),
                )
                .unwrap(),
        );
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
                drop(
                    input
                        .prepare(&Inspecting, set, &crate::PreparationLimits::default())
                        .unwrap(),
                );
                drop(
                    output
                        .prepare(&Inspecting, set, &crate::PreparationLimits::default())
                        .unwrap(),
                );
            }
            drop(
                output
                    .prepare(
                        &Inspecting,
                        &crate::ExternalSchemas::default(),
                        &crate::PreparationLimits::default(),
                    )
                    .unwrap(),
            );
            drop(
                output
                    .prepare(
                        &Inspecting,
                        &ExternalSchemas::default(),
                        &crate::PreparationLimits::default(),
                    )
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
        drop(
            input
                .prepare(&Inspecting, &first, &crate::PreparationLimits::default())
                .unwrap(),
        );
        drop(
            input
                .prepare(&Inspecting, &second, &crate::PreparationLimits::default())
                .unwrap(),
        );
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
    fn a_prepared_schema_keeps_no_space_of_a_set_it_never_received() {
        let proof = proof();
        let operation = proof.resolve_operation("op").unwrap();
        // Prepared without external resources and retaining its environment, hence the document.
        let keep = operation
            .output()
            .unwrap()
            .prepare(
                &Inspecting,
                &crate::ExternalSchemas::default(),
                &crate::PreparationLimits::default(),
            )
            .unwrap();
        let without = Arc::downgrade(proof.document.inner.resolution.get().unwrap());
        let other = snapshot(1);
        let prepared = operation
            .input()
            .unwrap()
            .prepare(&Inspecting, &other, &crate::PreparationLimits::default())
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
    fn concurrent_preparations_resolve_their_own_set() {
        /// Fails preparation if resolution crosses immutable external sets.
        struct Checking;
        impl SchemaEvaluator for Checking {
            type Validator = Retaining;
            fn prepare(
                &self,
                request: EvaluationRequest<'_>,
                limits: &PreparationLimits,
                control: &dyn WorkControl,
            ) -> Result<Retaining, PreparationFailure> {
                let environment = request.environment(limits, control)?;
                if let Some(resource) = request.external_schemas.iter().next() {
                    let target = environment.resolve(&environment.root(), &resource.uri)?;
                    if target.schema.value().text() != resource.document.text() {
                        return Err(PreparationFailure::new(
                            PreparationFailureKind::Implementation {
                                message: "the environment carries another set's snapshot".into(),
                            },
                        ));
                    }
                }
                Ok(Retaining(environment))
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
                        assert!(
                            input
                                .prepare(&Checking, set, &crate::PreparationLimits::default())
                                .is_ok()
                        );
                        if i % 5 == 0 {
                            assert!(
                                output
                                    .prepare(
                                        &Checking,
                                        &crate::ExternalSchemas::default(),
                                        &crate::PreparationLimits::default()
                                    )
                                    .is_ok()
                            );
                        }
                    }
                });
            }
        });
        assert!(sets.iter().all(|set| set.kept_spaces() == 1));
    }
}
