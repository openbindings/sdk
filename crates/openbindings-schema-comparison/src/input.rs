use openbindings::{JsonValue, Schema};
use std::{
    fmt,
    sync::{Arc, Mutex},
};

/// Reusable exact document context. Indexing is lazy and charged to the call
/// that needs it. Only completed successful entries are retained; refusals and
/// cancellation leave the context usable. No reference causes I/O.
#[derive(Clone)]
pub struct ComparisonContext {
    pub(crate) data: Arc<ContextData>,
    pub(crate) index: Arc<Mutex<crate::reference::Index>>,
}
#[derive(Clone)]
pub(crate) struct ContextData {
    pub(crate) document: JsonValue,
    pub(crate) base: Option<String>,
    pub(crate) resources: Vec<(String, JsonValue)>,
}

impl fmt::Debug for ComparisonContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComparisonContext").finish_non_exhaustive()
    }
}
impl ComparisonContext {
    /// Retain a document root, which may be a non-OBI source carrier.
    pub fn new(document: JsonValue) -> Self {
        Self {
            data: Arc::new(ContextData {
                document,
                base: None,
                resources: Vec::new(),
            }),
            index: Arc::default(),
        }
    }

    /// Set an absolute initial base for a standalone schema root. Ignored for
    /// an OpenBindings document root: its document resource cannot be named by
    /// a caller-supplied URI. Changing configuration starts a fresh index.
    #[must_use]
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        Arc::make_mut(&mut self.data).base = Some(base.into());
        self.index = Arc::default();
        self
    }
    /// Supply external content under an absolute URI with no nonempty fragment.
    /// Duplicate supplied identities fail comparison rather than being overwritten.
    /// Registration identity and reached canonical aliases follow the evaluator's
    /// `ExternalSchemas` policy; unused alias conflicts are allowed.
    #[must_use]
    pub fn with_resource(mut self, uri: impl Into<String>, document: JsonValue) -> Self {
        Arc::make_mut(&mut self.data)
            .resources
            .push((uri.into(), document));
        self.index = Arc::default();
        self
    }
    /// Select an exact schema, retaining this context independently of its owner.
    /// Detached schemas and selections outside configured schema positions
    /// have private call-local declarations;
    /// selecting a value never adds it to the persistent document index.
    pub fn schema(&self, schema: JsonValue) -> SchemaInput {
        SchemaInput {
            schema,
            context: self.clone(),
        }
    }
    /// Borrow the exact document root.
    pub fn document(&self) -> &JsonValue {
        &self.data.document
    }
    /// Estimate heap bytes this context keeps alive now.
    ///
    /// Includes the complete exact source arenas of the root and supplied
    /// resources, configured URI buffers, and completed reference indexes. Uses
    /// existing capacities without building indexes or doing comparison work.
    /// Each shared source arena is counted once within this context; clones
    /// return the same estimate. Summing estimates of separate owners, including
    /// a document and its context or two contexts sharing resources, overcounts
    /// their shared allocations.
    ///
    /// This is neither resident memory nor the bytes freed by dropping one handle.
    /// Excludes allocator overhead and Wasm memory growth. Tree node occupancy
    /// is estimated, as in [`openbindings::Document::retained_bytes`]. Later
    /// comparisons or source-location lookups can grow retained indexes; call
    /// again to include that growth. Concurrent lookups can change the estimate
    /// while it is being computed. This is not a hard memory admission bound.
    pub fn retained_bytes(&self) -> usize {
        use openbindings_internal_json::backend;
        let mut arenas = std::collections::BTreeSet::new();
        let sources = std::iter::once(&self.data.document)
            .chain(self.data.resources.iter().map(|(_, value)| value))
            .filter(|value| arenas.insert(backend::occurrence(value.view()).0))
            .map(backend::retained_bytes)
            .sum::<usize>();
        size_of::<ContextData>()
            + size_of::<Mutex<crate::reference::Index>>()
            + 4 * size_of::<usize>() // Two Arc headers.
            + sources
            + self.data.base.as_ref().map_or(0, String::capacity)
            + backend::vec_bytes(&self.data.resources)
            + self.data.resources.iter().map(|(uri, _)| uri.capacity()).sum::<usize>()
            + self.index.lock().expect("comparison index").retained_bytes()
    }
}
/// One exact schema and its reusable document context. An omitted slot is None.
#[derive(Clone)]
pub struct SchemaInput {
    pub(crate) schema: JsonValue,
    pub(crate) context: ComparisonContext,
}
impl fmt::Debug for SchemaInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SchemaInput").finish_non_exhaustive()
    }
}
impl SchemaInput {
    /// Retain a selected schema and its containing document root. For repeated
    /// comparisons, create inputs through one ComparisonContext instead.
    pub fn new(schema: JsonValue, document: JsonValue) -> Self {
        ComparisonContext::new(document).schema(schema)
    }
    /// Set the absolute starting base for a standalone schema root. Ignored for
    /// an OpenBindings document root, as in [`ComparisonContext::with_base`].
    #[must_use]
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.context = self.context.with_base(base);
        self
    }
    /// Supply exact external content without fetching it.
    #[must_use]
    pub fn with_resource(mut self, uri: impl Into<String>, document: JsonValue) -> Self {
        self.context = self.context.with_resource(uri, document);
        self
    }
    /// Borrow the selected exact schema without normalizing it.
    pub fn schema(&self) -> &JsonValue {
        &self.schema
    }
    /// Borrow the root used by fragment-only references.
    pub fn document(&self) -> &JsonValue {
        self.context.document()
    }
    /// Borrow the reusable context retained by this input.
    pub fn context(&self) -> &ComparisonContext {
        &self.context
    }
}
impl From<&Schema> for SchemaInput {
    fn from(schema: &Schema) -> Self {
        Self::new(schema.json(), schema.document().document().value().clone())
    }
}
