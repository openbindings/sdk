//! Read-only schema positions in their original document and external sources.
use crate::{schema::references::SchemaSpace, *};
pub(crate) use openbindings_internal_schema::schema_index::{
    DIALECT, MetaChecks, SameDocument, SchemaIndex,
};
use std::{fmt, sync::Arc};

pub(crate) struct EnvironmentInner {
    pub(crate) space: Arc<SchemaSpace>,
    pub(crate) root: usize,
    // Retain the immutable pair whose cache owns this resolution space.
    _document: Option<ConformantDocument>,
    _external: ExternalSchemas,
}
/// An owned inventory of original schemas. Construction performs no I/O and
/// does not resolve references eagerly. Clones belong to the same environment.
#[derive(Clone)]
pub struct SchemaEnvironment {
    pub(crate) inner: Arc<EnvironmentInner>,
}
/// An owned schema position in an environment. This can belong to a document,
/// supplied source, or packaged meta-schema; it is never a projected schema.
#[derive(Clone)]
pub struct SchemaNode {
    pub(crate) environment: SchemaEnvironment,
    pub(crate) id: usize,
    origin: SchemaOrigin,
}
/// Opaque identity of one resource in one environment. URI spelling alone is
/// insufficient: two environments may supply different schemas at that URI.
#[derive(Clone)]
pub struct ResourceIdentity {
    pub(crate) environment: SchemaEnvironment,
    pub(crate) id: usize,
}
impl PartialEq for ResourceIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && Arc::ptr_eq(&self.environment.inner, &other.environment.inner)
    }
}
impl Eq for ResourceIdentity {}
/// The reference-resolution base of a resource, distinct from its source URI.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ResourceBase<'a> {
    /// The OpenBindings document resource has no publicly named base.
    AnonymousDocument,
    /// Resolved original identifier spelling, used as a reference base.
    Named(&'a str),
    /// A declared identifier did not establish a usable base.
    Invalid,
}
macro_rules! structural_debug {
    ($($ty:ident),*) => {$(impl fmt::Debug for $ty {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct(stringify!($ty)).finish_non_exhaustive()
        }
    })*};
}
structural_debug!(
    EnvironmentInner,
    SchemaEnvironment,
    SchemaNode,
    ResourceIdentity
);
impl fmt::Debug for ResourceBase<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AnonymousDocument => "AnonymousDocument",
            Self::Named(_) => "Named(..)",
            Self::Invalid => "Invalid",
        })
    }
}
impl SchemaEnvironment {
    pub(crate) fn new(
        schema: &Schema,
        external: &ExternalSchemas,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self, PreparationFailure> {
        control.check()?;
        openbindings_internal_schema::preparation::Attempt::run(
            limits.max_sdk_work_units,
            limits.max_sdk_reservation_bytes,
            &|| control.is_cancelled(),
            |attempt| {
                Self::with_control(
                    schema,
                    external,
                    limits.max_schema_nodes,
                    limits.max_identity_bytes,
                    attempt,
                )
            },
        )
        .map_err(|failure| construction_failure(failure, schema))?
    }

    pub(crate) fn with_control(
        schema: &Schema,
        external: &ExternalSchemas,
        maximum_nodes: usize,
        maximum_identity: usize,
        control: &dyn openbindings_internal_schema::Control,
    ) -> Result<Self, PreparationFailure> {
        // Reserve the returned handle before construction can publish a
        // completed immutable graph to the document/resource-pair cache.
        control
            .charge_allocation(256)
            .map_err(|failure| construction_failure(failure, schema))?;
        let space = schema
            .document()
            .resolution_bounded(external, maximum_nodes, maximum_identity, control)
            .map_err(|failure| construction_failure(failure, schema))?;
        control
            .check()
            .map_err(|failure| construction_failure(failure, schema))?;
        let root = space
            .document_node(&schema.json())
            .expect("contained schema was admitted by conformance");
        Ok(Self {
            inner: Arc::new(EnvironmentInner {
                space,
                root,
                _document: Some(schema.document().clone()),
                _external: external.clone(),
            }),
        })
    }
    fn supplied_with_control(
        uri: &str,
        external: &ExternalSchemas,
        maximum_nodes: usize,
        maximum_identity: usize,
        control: &dyn openbindings_internal_schema::Control,
    ) -> Result<Self, PreparationFailure> {
        let index = external.resource_index(uri, maximum_identity, control)?;
        control.charge_allocation(256)?;
        let space = external.supplied_space(maximum_nodes, maximum_identity, control)?;
        control.check()?;
        let root = space.supplied_root(index);
        Ok(Self {
            inner: Arc::new(EnvironmentInner {
                space,
                root,
                _document: None,
                _external: external.clone(),
            }),
        })
    }
    /// The selected original schema, with an owner independent of this handle.
    pub fn root(&self) -> SchemaNode {
        self.node(self.inner.root)
    }
    pub(crate) fn node(&self, id: usize) -> SchemaNode {
        let at = self.inner.space.location(id);
        SchemaNode {
            environment: self.clone(),
            id,
            origin: SchemaOrigin::new(at.resource, at.pointer),
        }
    }
    pub(crate) fn owns(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

fn construction_failure(
    failure: openbindings_internal_schema::Failure,
    schema: &Schema,
) -> PreparationFailure {
    let mut failure = PreparationFailure::from(failure);
    // An exhausted attempt cannot spend an unbounded amount describing its
    // own failure. Limit/cancellation causes need no newly materialized path;
    // other fallback provenance has the same fixed diagnostic cap as queries.
    if failure.origin.is_none()
        && !matches!(
            failure.kind,
            PreparationFailureKind::Limit { .. } | PreparationFailureKind::Cancelled
        )
        && openbindings_internal_json::backend::location_pointer_size(
            schema.value.view(),
            64 * 1024,
        )
        .is_some()
    {
        failure.origin = Some(schema.location());
    }
    failure
}
impl SchemaNode {
    /// Exact original schema JSON, retaining its source snapshot.
    pub fn value(&self) -> JsonValue {
        self.environment.inner.space.node(self.id).value.clone()
    }
    /// Identity for dynamic-scope tracking within this environment.
    pub fn resource(&self) -> ResourceIdentity {
        ResourceIdentity {
            environment: self.environment.clone(),
            id: self.environment.inner.space.node(self.id).resource,
        }
    }
    /// Resolved reference base. Never combine this with the source-relative
    /// pointer to manufacture an original source address.
    pub fn resource_base(&self) -> ResourceBase<'_> {
        let resource = self
            .environment
            .inner
            .space
            .resource(self.environment.inner.space.node(self.id).resource);
        if resource.anonymous {
            ResourceBase::AnonymousDocument
        } else {
            resource
                .base
                .as_deref()
                .map_or(ResourceBase::Invalid, ResourceBase::Named)
        }
    }
    /// Effective resource dialect, including inheritance and the external
    /// 2020-12 default. Unknown valid URIs remain observable metadata. `None`
    /// denotes a declaration that was not an absolute scalar URI.
    pub fn dialect(&self) -> Option<&str> {
        self.environment
            .inner
            .space
            .resource(self.environment.inner.space.node(self.id).resource)
            .dialect
            .as_deref()
            .filter(|uri| crate::uri::absolute(uri))
    }
    /// Schema at this resource's root. The anonymous document resource has no
    /// schema root: the OpenBindings object is never fabricated as a schema.
    pub fn resource_root(&self) -> Option<Self> {
        self.environment
            .inner
            .space
            .resource(self.environment.inner.space.node(self.id).resource)
            .root
            .map(|id| self.environment.node(id))
    }
    /// Original source identity and exact pointer from that source's root.
    pub fn origin(&self) -> &SchemaOrigin {
        &self.origin
    }
    /// Exact source-relative pointer, including UTF-16 units where necessary.
    pub fn pointer(&self) -> &Pointer {
        &self.origin.pointer
    }
    /// Original source text containing this schema, with no copied projection.
    pub fn source(&self) -> JsonValue {
        let space = &self.environment.inner.space;
        space.source(space.node(self.id).source).value.clone()
    }
    /// Read an indexed subschema at decoded JSON Pointer segments. Literal
    /// values, maps of schemas, and unknown-keyword objects are not schemas.
    ///
    /// # Errors
    /// Returns caller misuse for a non-schema path, or a capability refusal
    /// when the resource's dialect does not establish this inventory.
    pub fn subschema(&self, path: &[&str]) -> Result<Self, ResolutionFailure> {
        let pointer: Pointer = path
            .iter()
            .map(|part| format!("/{}", part.replace('~', "~0").replace('/', "~1")))
            .collect::<String>()
            .parse()
            .expect("escaped pointer segments");
        self.subschema_at(&pointer)
    }
    /// Read a subschema at an exact relative pointer, including unpaired units.
    ///
    /// # Errors
    /// Same inventory and misuse failures as [`Self::subschema`].
    pub fn subschema_at(&self, pointer: &Pointer) -> Result<Self, ResolutionFailure> {
        let space = &self.environment.inner.space;
        let id = space.subschema_query(self.id, pointer).map_err(|error| {
            use openbindings_internal_schema::QueryFailure as Q;
            if matches!(error, Q::MissingTarget | Q::NonSchemaTarget) {
                ResolutionFailure {
                    kind: ResolutionFailureKind::NotSchemaLocation,
                    origin: Some(self.origin.clone()),
                }
            } else {
                self.environment.query_failure(error, self)
            }
        })?;
        Ok(self.environment.node(id))
    }
}
impl Schema {
    /// Inventory original schemas with explicit external inputs and bounded
    /// work. References are resolved on demand; nothing is fetched.
    ///
    /// # Errors
    /// Returns preparation cancellation or an inventory limit, not a value verdict.
    pub fn environment(
        &self,
        external: &ExternalSchemas,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<SchemaEnvironment, PreparationFailure> {
        SchemaEnvironment::new(self, external, limits, control)
    }
}
impl EvaluationRequest<'_> {
    /// Construct an original-schema environment for the selected root. A supplied
    /// root enters its own resource, under its own base and dialect, with the
    /// other explicit resources available for references. Nothing is fetched.
    ///
    /// # Errors
    /// Returns cancellation, an inventory limit, or an unavailable supplied root.
    pub fn environment(
        &self,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<SchemaEnvironment, PreparationFailure> {
        match self.root {
            SchemaRoot::Contained(schema) => {
                schema.environment(self.external_schemas, limits, control)
            }
            SchemaRoot::Supplied(uri) => {
                control.check()?;
                openbindings_internal_schema::preparation::Attempt::run(
                    limits.max_sdk_work_units,
                    limits.max_sdk_reservation_bytes,
                    &|| control.is_cancelled(),
                    |attempt| {
                        SchemaEnvironment::supplied_with_control(
                            uri,
                            self.external_schemas,
                            limits.max_schema_nodes,
                            limits.max_identity_bytes,
                            attempt,
                        )
                    },
                )
                .map_err(PreparationFailure::from)?
            }
        }
    }
}

impl openbindings_internal_schema::OriginalEnvironment for SchemaEnvironment {
    fn space(&self) -> &openbindings_internal_schema::SchemaSpace {
        &self.inner.space
    }
    fn entry(&self) -> usize {
        self.inner.root
    }
}

impl openbindings_internal_schema::preparation::EnvironmentRequest for EvaluationRequest<'_> {
    type Environment = SchemaEnvironment;
    type Failure = PreparationFailure;
    fn environment_with_control(
        &self,
        maximum_nodes: usize,
        maximum_identity: usize,
        control: &dyn openbindings_internal_schema::Control,
    ) -> Result<Self::Environment, Self::Failure> {
        match self.root {
            SchemaRoot::Contained(schema) => SchemaEnvironment::with_control(
                schema,
                self.external_schemas,
                maximum_nodes,
                maximum_identity,
                control,
            ),
            SchemaRoot::Supplied(uri) => SchemaEnvironment::supplied_with_control(
                uri,
                self.external_schemas,
                maximum_nodes,
                maximum_identity,
                control,
            ),
        }
    }
}
