use openbindings::{JsonValue, Schema};
use std::fmt;

/// One exact schema and the caller-supplied context for profile reference inlining.
/// Construction retains input; normalization validates used schema positions and
/// reference configuration. An omitted slot is `None`, never an empty schema.
#[derive(Clone)]
pub struct SchemaInput {
    pub(crate) schema: JsonValue,
    pub(crate) document: JsonValue,
    pub(crate) base: Option<String>,
    pub(crate) resources: Vec<(String, JsonValue)>,
}
impl fmt::Debug for SchemaInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SchemaInput").finish_non_exhaustive()
    }
}
impl SchemaInput {
    /// Retain a selected schema and its containing document root. They need not
    /// form a conformant OBI; an offered schema can be compared before insertion.
    pub fn new(schema: JsonValue, document: JsonValue) -> Self {
        Self {
            schema,
            document,
            base: None,
            resources: Vec::new(),
        }
    }
    /// Set the absolute starting base for this profile input. A schema resource's
    /// `$id` establishes its own base. This is not an OBI acquisition-base declaration.
    #[must_use]
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = Some(base.into());
        self
    }
    /// Supply exact external content under an absolute URI without a nonempty
    /// fragment. Resolution performs no I/O. Duplicate identities are refused
    /// during comparison, not overwritten. The URI becomes that resource's base.
    #[must_use]
    pub fn with_resource(mut self, uri: impl Into<String>, document: JsonValue) -> Self {
        self.resources.push((uri.into(), document));
        self
    }
    /// Borrow the selected exact schema without normalizing it.
    pub fn schema(&self) -> &JsonValue {
        &self.schema
    }
    /// Borrow the document root used by fragment-only references.
    pub fn document(&self) -> &JsonValue {
        &self.document
    }
}
impl From<&Schema> for SchemaInput {
    fn from(schema: &Schema) -> Self {
        Self::new(schema.json(), schema.document().document().value().clone())
    }
}
