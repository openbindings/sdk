//! Queries against original sources, independent of evaluator projections.
use super::*;

/// Structural query results, without diagnostic-string classification.
#[derive(Clone, Debug, PartialEq, Eq)]
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

impl SchemaSpace {
    /// Resolve any URI reference, including a dialect URI that is not the
    /// value of a reference keyword. No registry projection or I/O is involved.
    /// Unknown dialect roots are observable; dialect-dependent inventory
    /// queries refuse rather than assuming 2020-12 positions.
    pub fn query(&self, holder: usize, reference: &str) -> Result<Target, QueryFailure> {
        if !uri::valid(reference) {
            return Err(QueryFailure::MalformedReference);
        }
        self.query_identifier_interpretation(holder)?;
        let current = self.resource(self.node(holder).resource);
        if !self.eligible(self.node(holder).resource) {
            return Err(QueryFailure::IdentityConflict);
        }
        let resource = if current.anonymous && !uri::absolute(reference) {
            if !reference.is_empty() && !reference.starts_with('#') {
                return Err(QueryFailure::InvalidIdentifier);
            }
            self.node(holder).resource
        } else {
            let absolute = uri::compared_id(current.base.as_deref(), reference)
                .ok_or(QueryFailure::InvalidIdentifier)?;
            self.named_resource(&absolute)?
        };
        self.query_fragment(resource, reference)
    }

    /// One selection implementation for ordinary references and custom dialect
    /// certificates. A certificate does not acquire a second alias registry.
    pub(super) fn named_resource(&self, absolute: &str) -> Result<usize, QueryFailure> {
        if !uri::absolute(absolute) {
            return Err(QueryFailure::MalformedReference);
        }
        let name = absolute.split_once('#').map_or(absolute, |(name, _)| name);
        let exact = self.name(name).filter(|name| name.exact_contained);
        let normalized =
            || crate::input_policy::canonical_uri(name).and_then(|name| self.name(&name));
        let selected = exact.or_else(normalized);
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
            }) if self.eligible(resource) => Ok(resource),
            Some(_) => Err(QueryFailure::IdentityConflict),
            None => Err(QueryFailure::ResourceUnavailable(name.into())),
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
