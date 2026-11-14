//! Queries against original sources, independent of evaluator projections.
use super::*;
#[cfg(test)]
#[path = "identifier_tests.rs"]
mod identifier_tests;

/// Structural query results, without diagnostic-string classification.
#[derive(Clone, PartialEq, Eq)]
pub enum QueryFailure {
    InvalidExternalSchema,
    MalformedReference,
    MissingTarget,
    NonSchemaTarget,
    MissingAnchor,
    AmbiguousAnchor,
    InvalidIdentifier,
    IdentityConflict,
    ResourceUnavailable(String),
    UnsupportedDialect(Option<String>),
    UnsupportedCapability(&'static str),
}

impl std::fmt::Debug for QueryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidExternalSchema => "InvalidExternalSchema",
            Self::MalformedReference => "MalformedReference",
            Self::MissingTarget => "MissingTarget",
            Self::NonSchemaTarget => "NonSchemaTarget",
            Self::MissingAnchor => "MissingAnchor",
            Self::AmbiguousAnchor => "AmbiguousAnchor",
            Self::InvalidIdentifier => "InvalidIdentifier",
            Self::IdentityConflict => "IdentityConflict",
            Self::ResourceUnavailable(..) => "ResourceUnavailable { .. }",
            Self::UnsupportedDialect(..) => "UnsupportedDialect { .. }",
            Self::UnsupportedCapability(..) => "UnsupportedCapability { .. }",
        })
    }
}

/// Compact identifier work owned by one inspection or preparation run.
/// Dropping this resolver releases its query inputs, even when the source
/// environment is kept for later requests. It does not cache query results.
pub struct ReferenceResolver<'a> {
    space: &'a SchemaSpace,
    pub(super) names: Option<Box<names::Names<'a>>>,
}
impl std::fmt::Debug for ReferenceResolver<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReferenceResolver").finish_non_exhaustive()
    }
}

impl SchemaSpace {
    pub fn reference_resolver(&self) -> ReferenceResolver<'_> {
        ReferenceResolver {
            space: self,
            names: None,
        }
    }
    /// Resolve one reference without retaining its query inputs in the space.
    pub fn query(&self, holder: usize, reference: &str) -> Result<Target, QueryFailure> {
        self.reference_resolver().query(holder, reference)
    }
}

impl ReferenceResolver<'_> {
    pub fn space(&self) -> &SchemaSpace {
        self.space
    }
    pub(super) fn resolve_with_missing(
        &mut self,
        holder: usize,
        reference: &str,
        missing: &mut impl FnMut(&str),
    ) -> Result<Target, Failure> {
        SchemaSpace::resolve_query(self.query(holder, reference), missing).map_err(|mut failure| {
            failure.location = Some(self.space.location(holder));
            failure
        })
    }
    /// Resolve any URI reference, including a dialect URI that is not the
    /// value of a reference keyword. No registry projection or I/O is involved.
    /// Unknown dialect roots are observable; dialect-dependent inventory
    /// queries refuse rather than assuming 2020-12 positions.
    pub fn query(&mut self, holder: usize, reference: &str) -> Result<Target, QueryFailure> {
        self.query_controlled(holder, reference, &|| false)
            .expect("unbounded query control")
    }

    /// Reserve compact query input and any owned missing name before doing the
    /// corresponding work. The environment already reserves its identity index.
    pub fn query_controlled(
        &mut self,
        holder: usize,
        reference: &str,
        control: &dyn Control,
    ) -> Result<Result<Target, QueryFailure>, Failure> {
        self.query_admitted(holder, reference, control, false, &mut |_, _| Ok(()))
    }

    /// Admit the entire report entry before spelling a missing identity. Even
    /// absolute missing names use compact handles on this path.
    pub(super) fn query_for_report(
        &mut self,
        holder: usize,
        reference: &str,
        admit: &mut impl FnMut(&Result<Target, QueryFailure>, usize) -> Result<(), Failure>,
    ) -> Result<Result<Target, QueryFailure>, Failure> {
        self.query_admitted(holder, reference, &|| false, true, admit)
    }

    fn query_admitted(
        &mut self,
        holder: usize,
        reference: &str,
        control: &dyn Control,
        compact_missing: bool,
        admit: &mut impl FnMut(&Result<Target, QueryFailure>, usize) -> Result<(), Failure>,
    ) -> Result<Result<Target, QueryFailure>, Failure> {
        let space = self.space;
        let reserve = |length: usize, work: usize, reservation: usize| -> Result<(), Failure> {
            let bytes = length
                .checked_mul(16)
                .and_then(|n| n.checked_add(reservation))
                .ok_or_else(|| {
                    control.allocation_limit_reached();
                    Failure::new(
                        FailureReason::LimitExceeded,
                        "preparation-sdk-reservation-bytes",
                        "reference resolution reservation overflow",
                    )
                })?;
            control.charge_work(bytes - reservation + work)?;
            control.charge_allocation(bytes)
        };
        reserve(reference.len(), 1, 512)?;
        let mut query = || -> Result<_, QueryFailure> {
            if !uri::valid(reference) {
                return Err(QueryFailure::MalformedReference);
            }
            space.query_identifier_interpretation(holder)?;
            let resource = space.node(holder).resource;
            let current = space.resource(resource);
            if !space.eligible(resource) {
                return Err(QueryFailure::IdentityConflict);
            }
            if current.anonymous && !uri::absolute(reference) {
                if !reference.is_empty() && !reference.starts_with('#') {
                    return Err(QueryFailure::InvalidIdentifier);
                }
                return Ok(Ok(resource));
            }
            // Eligibility already proves this carrier owns its base name.
            // A fragment-only reference preserves that name. This also avoids
            // building a namespace index for a deep chain with one local ref.
            if current.fragment_base && (reference.is_empty() || reference.starts_with('#')) {
                return space
                    .selected_resource(Some(NameSelection {
                        origin: current.origin,
                        carrier: Some(resource),
                        exact_contained: current.origin == ResourceOrigin::Contained,
                    }))
                    .map(|selected| Ok(selected.expect("eligible base carrier")));
            }
            // These resolutions cannot inherit any base text. Keep the string
            // resolver's input-sized path and avoid compact setup altogether.
            if !compact_missing && (current.base.is_none() || uri::absolute(reference)) {
                let name =
                    uri::compared_id(None, reference).ok_or(QueryFailure::InvalidIdentifier)?;
                return space.named_resource(&name).map(Ok);
            }
            let names = self
                .names
                .get_or_insert_with(|| Box::new(names::Names::new(space)));
            let (handle, selected) = names.select(resource, current.base.as_deref(), reference)?;
            space
                .selected_resource(selected)
                .map(|selected| match selected {
                    Some(resource) => Ok(resource),
                    None => Err(handle),
                })
        };
        let result = match query() {
            Err(cause) => Err(cause),
            Ok(Ok(resource)) => space.query_fragment(resource, reference),
            Ok(Err(handle)) => {
                let names = self.names.as_ref().expect("queried name inventory");
                admit(
                    &Err(QueryFailure::ResourceUnavailable(String::new())),
                    names.length(handle),
                )?;
                reserve(names.length(handle), 0, 0)?;
                return Ok(Err(QueryFailure::ResourceUnavailable(names.spell(handle))));
            }
        };
        admit(&result, 0)?;
        Ok(result)
    }
}

impl SchemaSpace {
    /// One selection implementation for ordinary references and custom dialect
    /// certificates. A certificate does not acquire a second alias registry.
    pub(super) fn named_resource(&self, absolute: &str) -> Result<usize, QueryFailure> {
        if !uri::absolute(absolute) {
            return Err(QueryFailure::MalformedReference);
        }
        let name = absolute.split_once('#').map_or(absolute, |(name, _)| name);
        // Registered keys are canonical aliases or exact contained spellings.
        // An exact contained key already wins; any other hit is canonical and
        // canonicalization is idempotent, so resolving it again cannot change
        // selection (including a conflict).
        let selected = self
            .name(name)
            .or_else(|| crate::input_policy::canonical_uri(name).and_then(|name| self.name(&name)));
        self.selected_resource(selected)?
            .ok_or_else(|| QueryFailure::ResourceUnavailable(name.into()))
    }

    fn selected_resource(
        &self,
        selected: Option<NameSelection>,
    ) -> Result<Option<usize>, QueryFailure> {
        // Additional opaque claims cannot erase a conflict already established
        // by the original carriers. Preserve that determinate evidence before
        // refusing selections that would require proving namespace absence.
        if selected.is_some_and(|selection| {
            selection
                .carrier
                .is_none_or(|resource| !self.eligible(resource))
        }) {
            return Err(QueryFailure::IdentityConflict);
        }
        if self.local.uncertain_identifiers
            && selected.is_none_or(|s| s.origin != ResourceOrigin::Contained)
        {
            return Err(QueryFailure::UnsupportedCapability(
                "schema-identifier-form",
            ));
        }
        if self.custom.unknown_namespace
            && selected.is_none_or(|s| s.origin != ResourceOrigin::Contained)
        {
            return Err(QueryFailure::UnsupportedCapability(
                "custom-profile-namespace",
            ));
        }
        match selected {
            Some(NameSelection {
                carrier: Some(resource),
                ..
            }) if self.eligible(resource) => Ok(Some(resource)),
            Some(_) => Err(QueryFailure::IdentityConflict),
            None => Ok(None),
        }
    }

    /// Whether this resource's non-root schema positions are known by this
    /// inventory. A custom evaluator can still inspect its root and meta-schema.
    pub fn query_inventory(&self, resource: usize) -> Result<(), QueryFailure> {
        if let Some(Err(cause)) = self.custom_meta(resource) {
            return Err(cause);
        }
        if self.local.uncertain_identifiers
            && self.resource(resource).origin != ResourceOrigin::Contained
        {
            return Err(QueryFailure::UnsupportedCapability(
                "schema-identifier-form",
            ));
        }
        let dialect = &self.resource(resource).dialect;
        if self.resource(resource).profile.is_some() {
            Ok(())
        } else {
            Err(QueryFailure::UnsupportedDialect(
                dialect.clone().filter(|uri| uri::absolute(uri)),
            ))
        }
    }

    fn query_identifier_interpretation(&self, node: usize) -> Result<(), QueryFailure> {
        let original = self.node(node);
        if original.profile.is_none()
            && self
                .resource(original.resource)
                .dialect
                .as_deref()
                .and_then(crate::dialect::Dialect::from_uri)
                .is_some()
        {
            return Err(QueryFailure::UnsupportedCapability(
                "schema-identifier-form",
            ));
        }
        Ok(())
    }

    pub(super) fn query_fragment(
        &self,
        resource_id: usize,
        reference: &str,
    ) -> Result<Target, QueryFailure> {
        let resource = self.resource(resource_id);
        let fragment = reference
            .split_once('#')
            .map_or("", |(_, fragment)| fragment);
        let decoded = uri::decode_fragment(fragment).ok_or(QueryFailure::MalformedReference)?;
        if decoded.is_empty() {
            return resource
                .root
                .filter(|node| {
                    matches!(
                        self.node(*node).value.kind(),
                        JsonKind::Object | JsonKind::Boolean
                    )
                })
                .map(|node| Target {
                    node,
                    dynamic_name: None,
                })
                .ok_or(QueryFailure::NonSchemaTarget);
        }
        self.query_inventory(resource_id)?;
        if !decoded.starts_with('/') {
            return match resource.anchors.get(&decoded).map(Vec::as_slice) {
                Some([(node, dynamic)]) => Ok(Target {
                    node: *node,
                    dynamic_name: dynamic.then_some(decoded),
                }),
                Some(_) => Err(QueryFailure::AmbiguousAnchor),
                None => Err(QueryFailure::MissingAnchor),
            };
        }
        // Parsing the pointer is distinct from not finding its target.
        let pointer: openbindings_internal_json::Pointer = decoded
            .parse()
            .map_err(|_| QueryFailure::MalformedReference)?;
        let (root, source) = match resource.root {
            Some(root) => {
                let node = self.node(root);
                (node.value.view(), self.source(node.source))
            }
            None if resource.anonymous => {
                let source = &self.local.sources[0];
                (source.value.view(), source)
            }
            None => return Err(QueryFailure::NonSchemaTarget),
        };
        let node = self.query_pointer(root, source, &pointer)?;
        if !self.eligible(self.node(node).resource) {
            return Err(QueryFailure::IdentityConflict);
        }
        // A pointer may cross into a nested resource. Its root remains visible
        // with an unknown dialect; locations below it must not inherit the
        // referring resource's inventory rules.
        let target = self.resource(self.node(node).resource);
        if target.root != Some(node) {
            self.query_inventory(self.node(node).resource)?;
        }
        Ok(Target {
            node,
            dynamic_name: None,
        })
    }
    /// Checked original traversal, preserving exact UTF-16 pointer units.
    pub fn subschema_query(
        &self,
        holder: usize,
        pointer: &openbindings_internal_json::Pointer,
    ) -> Result<usize, QueryFailure> {
        let node = self.node(holder);
        self.query_pointer(node.value.view(), self.source(node.source), pointer)
    }
    fn query_pointer(
        &self,
        root: JsonRef<'_>,
        source: &Source,
        pointer: &openbindings_internal_json::Pointer,
    ) -> Result<usize, QueryFailure> {
        let mut value = root;
        let units: Vec<u16> = pointer.utf16_units().collect();
        if !units.is_empty() {
            for token in units[1..].split(|unit| *unit == u16::from(b'/')) {
                if let Some(id) = source.positions.get(&ref_id(value)) {
                    self.query_inventory(self.node(*id).resource)?;
                    self.query_identifier_interpretation(*id)?;
                }
                let mut path = Vec::with_capacity(token.len() + 1);
                path.push(u16::from(b'/'));
                path.extend_from_slice(token);
                let part = openbindings_internal_json::Pointer::from_utf16(&path)
                    .expect("one validated pointer token");
                value = value.at_pointer(&part).ok_or(QueryFailure::MissingTarget)?;
            }
        }
        let node = source
            .positions
            .get(&ref_id(value))
            .copied()
            .filter(|_| matches!(value.kind(), JsonKind::Object | JsonKind::Boolean))
            .ok_or(QueryFailure::NonSchemaTarget)?;
        if !self.eligible(self.node(node).resource) {
            return Err(QueryFailure::IdentityConflict);
        }
        Ok(node)
    }
}
