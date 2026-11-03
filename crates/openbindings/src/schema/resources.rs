//! Explicit external schemas, with immutable identity and lifetime boundaries.
use crate::{document::DocumentInner, schema::references::SchemaSpace, *};
use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, Mutex, PoisonError, Weak},
};
#[derive(Clone)]
/// An explicit immutable schema resource associated with its retrieval identity. Supplying it performs no I/O.
pub struct ExternalSchema {
    /// Absolute retrieval URI without a nonempty fragment; [`ExternalSchemas::new`] validates and normalizes identity.
    pub uri: String,
    /// Exact resource JSON retained by its set; duplicate-member resources are refused.
    pub document: JsonValue,
}
/// Immutable caller-supplied resources. Separate sets can use the same URI
/// for different snapshots; this is not a process-wide registry or an acquirer.
/// Clones share storage, including the identity index built by [`Self::new`].
/// [`SchemaRequest::evaluation_program`] reads that index, and the resolution
/// space it assembles for a nonempty set and a document revision is kept with
/// the set while both are alive, and released when either is dropped. The
/// default evaluator reuses the index instead of indexing the set again, but
/// assembles its own resolution space for each preparation, so its preparation
/// time still grows with the set's resources, including those the value contract
/// never reaches.
#[derive(Clone, Default)]
pub struct ExternalSchemas {
    inner: Arc<ExternalSchemasInner>,
}
#[derive(Default)]
pub(crate) struct ExternalSchemasInner {
    resources: Vec<ExternalSchema>,
    input_policy: openbindings_internal_schema::Supplied,
    // One space per live revision this set was projected with. A revision's
    // entry leaves when the revision is dropped; all leave with the set.
    spaces: Mutex<Vec<(Weak<DocumentInner>, Arc<SchemaSpace>)>>,
}
impl Drop for ExternalSchemasInner {
    fn drop(&mut self) {
        let set: *const Self = self;
        let spaces = std::mem::take(
            self.spaces
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner),
        );
        // Live revisions stop tracking this set; the spaces go with it.
        for (revision, _) in &spaces {
            if let Some(revision) = revision.upgrade() {
                revision.forget_set(set);
            }
        }
    }
}
impl ExternalSchemasInner {
    /// Release the space kept for a revision that is being dropped.
    pub(crate) fn release(&self, document: *const DocumentInner) {
        // Drop the released space after unlocking: it can own large snapshots.
        let released: Vec<_> = {
            let mut spaces = self.spaces.lock().unwrap_or_else(PoisonError::into_inner);
            let (released, kept) = std::mem::take(&mut *spaces)
                .into_iter()
                .partition(|(revision, _)| std::ptr::eq(revision.as_ptr(), document));
            *spaces = kept;
            released
        };
        drop(released);
    }
}
/// Cause of external-schema registration failure, separate from conformance.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExternalSchemaErrorKind {
    /// Retrieval identity is not an absolute URI without a nonempty fragment.
    InvalidIdentity,
    /// A retrieval identity was registered twice.
    RepeatedIdentity,
    /// Exact JSON repeats object members.
    DuplicateMembers,
    /// A configured inventory or identity-expansion bound was reached.
    Limit {
        /// Resource whose bound was reached.
        resource: String,
    },
    /// The caller cancelled registration.
    Cancelled,
}
#[derive(Clone, PartialEq, Eq)]
/// Invalid supplied-resource configuration, separate from schema preparation and value outcomes.
/// Later versions may add fields: read them by name and match with `..`.
#[non_exhaustive]
pub struct ExternalSchemaError {
    /// Structured registration failure; never a schema verdict.
    pub kind: ExternalSchemaErrorKind,
    /// Caller-supplied resource identifier associated with the error; treat it as untrusted display data.
    pub uri: String,
    /// Explanation of the configuration failure; not a schema verdict.
    pub message: String,
}
impl fmt::Display for ExternalSchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ExternalSchemaError {}
impl ExternalSchemas {
    /// Return another immutable set with an explicit external schema.
    /// Neither set fetches content or changes when the other is prepared.
    ///
    /// # Errors
    /// Invalid or duplicate retrieval identities and ambiguous JSON members are
    /// refused without modifying this set.
    #[must_use = "the original external schemas remain unchanged"]
    pub fn with(&self, uri: &str, schema: JsonValue) -> Result<Self, ExternalSchemaError> {
        Self::new(self.iter().cloned().chain(std::iter::once(ExternalSchema {
            uri: uri.to_owned(),
            document: schema,
        })))
    }
    /// Admit immutable resources without acquisition. Duplicate retrieval spellings
    /// after RFC resolution and empty-fragment removal are configuration errors.
    /// Canonical associations use the library's ordinary URI normalization;
    /// collisions and protected standard identities are retained as input policy
    /// and refuse when reached, without poisoning unrelated contracts.
    ///
    /// # Errors
    /// Returns a resource error if a URI is not absolute or has a nonempty fragment, two resources have the same normalized URI, or a resource contains duplicate members.
    pub fn new(
        resources: impl IntoIterator<Item = ExternalSchema>,
    ) -> Result<Self, ExternalSchemaError> {
        Self::new_with_limits(
            resources,
            &PreparationLimits::default(),
            &crate::control::NeverCancelled,
        )
    }
    /// Register under caller-selected inventory and identity budgets, with
    /// cooperative cancellation. Failed attempts retain no set.
    ///
    /// # Errors
    /// Returns invalid/duplicate identity, duplicate members, limits, or cancellation.
    pub fn new_with_limits(
        resources: impl IntoIterator<Item = ExternalSchema>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self, ExternalSchemaError> {
        let mut by_uri = BTreeMap::new();
        let mut retrieval_bytes = 0usize;
        for mut resource in resources {
            let exceeded = |resource: &str| ExternalSchemaError {
                kind: ExternalSchemaErrorKind::Limit {
                    resource: resource.into(),
                },
                uri: String::new(),
                message: "external schema registration exceeds its configured budget".into(),
            };
            if by_uri.len() >= limits.max_schema_nodes {
                return Err(exceeded("schema-nodes"));
            }
            retrieval_bytes = retrieval_bytes
                .checked_add(resource.uri.len())
                .filter(|bytes| *bytes <= limits.max_identity_bytes)
                .ok_or_else(|| exceeded("schema-identity-bytes"))?;
            if control.is_cancelled() {
                return Err(ExternalSchemaError {
                    kind: ExternalSchemaErrorKind::Cancelled,
                    uri: String::new(),
                    message: "the caller cancelled registration".into(),
                });
            }
            let invalid = |message: &str| ExternalSchemaError {
                kind: ExternalSchemaErrorKind::InvalidIdentity,
                uri: resource.uri.clone(),
                message: message.into(),
            };
            if !crate::uri::absolute(&resource.uri)
                || crate::uri::fragment(&resource.uri).is_some_and(|s| !s.is_empty())
            {
                return Err(invalid(
                    "retrieval URI must be absolute with no nonempty fragment",
                ));
            }
            if resource.document.has_duplicate_names() {
                let mut error = invalid("resource repeats JSON member names");
                error.kind = ExternalSchemaErrorKind::DuplicateMembers;
                return Err(error);
            }
            resource.uri = crate::uri::compared_id(None, &resource.uri)
                .ok_or_else(|| invalid("invalid retrieval URI"))?;
            resource.document = openbindings_internal_json::backend::standalone(resource.document);
            let uri = resource.uri.clone();
            if by_uri.insert(uri.clone(), resource).is_some() {
                return Err(ExternalSchemaError {
                    kind: ExternalSchemaErrorKind::RepeatedIdentity,
                    uri,
                    message: "duplicate retrieval URI".into(),
                });
            }
        }
        let resources: Vec<_> = by_uri.into_values().collect();
        let input_policy = openbindings_internal_schema::Supplied::new_bounded(
            resources
                .iter()
                .map(|r| openbindings_internal_schema::ProjectedResource {
                    uri: r.uri.clone(),
                    document: r.document.clone(),
                }),
            limits.max_schema_nodes,
            limits.max_identity_bytes,
            &|| control.is_cancelled(),
        )
        .map_err(|failure| ExternalSchemaError {
            kind: if failure.reason == openbindings_internal_schema::FailureReason::Cancelled {
                ExternalSchemaErrorKind::Cancelled
            } else {
                ExternalSchemaErrorKind::Limit {
                    resource: failure.code,
                }
            },
            uri: String::new(),
            message: failure.message,
        })?;
        Ok(Self {
            inner: Arc::new(ExternalSchemasInner {
                resources,
                input_policy,
                spaces: Mutex::default(),
            }),
        })
    }
    /// Borrow supplied resources in deterministic normalized-URI order; no allocation or acquisition.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ExternalSchema> {
        self.inner.resources.iter()
    }
    /// Locate a schema location in one of these resources: the node its
    /// pointer names in the resource with its URI, with line and column in
    /// that resource's own text. A resource built from a subtree of a larger
    /// value is admitted as its own text, so its coordinates count from the
    /// subtree's start, where [`JsonValue::locate`] on that subtree counts in
    /// the whole text it was parsed from. `None` for a document location, a
    /// URI this set does not hold (such as a packaged meta-schema's), or a
    /// pointer that names no node. Positions are computed on this call.
    pub fn locate(&self, at: &SchemaOrigin) -> Option<SourceLocation> {
        let uri = at.source_uri.as_deref()?;
        let resources = &self.inner.resources;
        let found = resources
            .binary_search_by(|resource| resource.uri.as_str().cmp(uri))
            .ok()?;
        resources[found].document.locate(&at.pointer)
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.inner.resources.is_empty()
    }
    /// The space kept for this set and a live revision, if one was assembled.
    pub(crate) fn kept_space(&self, document: &Arc<DocumentInner>) -> Option<Arc<SchemaSpace>> {
        let spaces = self
            .inner
            .spaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        spaces
            .iter()
            .find(|(revision, _)| std::ptr::eq(revision.as_ptr(), Arc::as_ptr(document)))
            .map(|(_, space)| space.clone())
    }
    /// Keep `space` for this set and a live revision; when a concurrent
    /// preparation kept one first, that space wins and is returned.
    pub(crate) fn keep_space(
        &self,
        document: &Arc<DocumentInner>,
        space: Arc<SchemaSpace>,
    ) -> Arc<SchemaSpace> {
        let mut spaces = self
            .inner
            .spaces
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some((_, kept)) = spaces
            .iter()
            .find(|(revision, _)| std::ptr::eq(revision.as_ptr(), Arc::as_ptr(document)))
        {
            return kept.clone();
        }
        spaces.push((Arc::downgrade(document), space.clone()));
        space
    }
    pub(crate) fn downgrade(&self) -> Weak<ExternalSchemasInner> {
        Arc::downgrade(&self.inner)
    }
    #[cfg(test)]
    pub(crate) fn kept_spaces(&self) -> usize {
        self.inner.spaces.lock().unwrap().len()
    }
    /// One shared empty set, so preparation without resources allocates nothing.
    pub(crate) fn empty() -> &'static Self {
        static EMPTY: std::sync::OnceLock<ExternalSchemas> = std::sync::OnceLock::new();
        EMPTY.get_or_init(Self::default)
    }
}

// The default evaluator reuses the identity index built in `ExternalSchemas::new`
// rather than admitting the resources again on every preparation.
impl openbindings_internal_schema::SuppliedSet for ExternalSchemas {
    fn supplied(&self) -> &openbindings_internal_schema::Supplied {
        &self.inner.input_policy
    }
}

impl std::fmt::Debug for ExternalSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalSchema").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ExternalSchemas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalSchemas")
            .field("resources", &self.inner.resources.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ExternalSchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reason = match self.message.as_str() {
            "retrieval URI must be absolute with no nonempty fragment" => {
                "retrieval URI must be absolute with no nonempty fragment"
            }
            "resource repeats JSON member names" => "resource repeats JSON member names",
            "invalid retrieval URI" => "invalid retrieval URI",
            "duplicate retrieval URI" => "duplicate retrieval URI",
            _ => "<redacted>",
        };
        f.debug_struct("ExternalSchemaError")
            .field("reason", &reason)
            .finish_non_exhaustive()
    }
}
