//! Evaluator-private bounded preparation. No product API exposes this graph.
mod bounds;
use embedding::schema_index::DIALECT;
#[cfg(test)]
use embedding::schema_index::SchemaIndex;
use embedding::{
    Control as WorkControl, Failure as NoVerdict, FailureReason as NoVerdictReason,
    Location as SchemaLocation, Program as EvaluationProgram, ProjectedResource as SchemaResource,
    RawDocument as Document, Supplied as ResourceSet,
};
use openbindings_internal_json::backend::node_id;
use openbindings_internal_schema::{self as embedding, *};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    ops::{Deref, DerefMut},
};
pub(super) struct SchemaSpace(embedding::SchemaSpace);
impl SchemaSpace {
    fn new(document: Document, supplied: ResourceSet) -> Self {
        Self(embedding::SchemaSpace::new(document, supplied))
    }
}
impl Deref for SchemaSpace {
    type Target = embedding::SchemaSpace;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for SchemaSpace {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
/// Paired, owned JSON Schema validity bounds for one immutable resource snapshot.
///
/// For every admitted instance and every admissible completion that preserves
/// known resource identities, `lower(value) <= actual(value) <= upper(value)`.
/// **Neither program alone is an equivalent of the original partial schema.**
/// Adapters may establish failure from upper failure, satisfaction from upper
/// and lower success, and otherwise return [`Self::unavailable`] as no verdict.
/// They must not promote an incomplete, cancelled or unsupported evaluation.
/// There is no annotation-output guarantee, nor a promise that arbitrary later
/// resources (for example colliding identities or unsupported dialects) prepare.
///
/// Hole influence is permitted through static `$ref`, `allOf`, `anyOf`,
/// `properties`, `patternProperties`, `additionalProperties`, `propertyNames`,
/// `dependentSchemas`, `prefixItems` and `items`. Hole-dependent `oneOf` uses
/// memoized dual-polarity bounds, not uniform false/true substitution. Closed
/// `not`, effective conditionals and `contains` remain supported; holes beneath
/// those operators refuse. Correlations between unknown predicates are not proved.
/// Evaluated `unevaluated*`, `$dynamicRef` and `$dynamicAnchor` refuse this helper.
/// Ignored/opaque positions remain ignored. Potential in-place cycles refuse;
/// advancing recursion is bounded by the admitted finite instance.
///
/// Admission: depth 256, 100,000 reached nodes, 200,000 dependency edges (including
/// holes), 100,000 holes, and 64 MiB combined retained UTF-8 text. Text includes
/// both projected JSON resources' serialized text and separately owned decoded
/// strings, their URI/entry strings, shared original maps once, and evidence
/// strings, including private exact/prefix/barrier mapping entries. Dependent
/// oneOf additionally admits 100,000 emitted schema-node occurrences and 200,000
/// reference/applicator-edge occurrences across both independently closed copies.
/// Wrappers, ordinary bodies, constants and branch refs count. These are unmeasured
/// admission choices, not heap, allocator-capacity, compiled-memory or runtime bounds. Reference-resolution scratch inputs and full original
/// pointers also have a separate 64 MiB pre-copy guard. Refusal locations outside
/// successful plans use that guard too (including any keyword suffix), and can
/// be omitted or identify a bounded original preparation boundary. No I/O occurs. Later
/// resource replacements require a new preparation; these bounds never change.
#[derive(Debug)]
pub(super) struct EvaluationBounds {
    pub(crate) lower: EvaluationProgram,
    pub(crate) upper: EvaluationProgram,
    pub(crate) unavailable: Option<NoVerdict>,
    /// The projections themselves are admissible all-false/all-true
    /// completions of proven semantic holes, with no absent carriers.
    pub(crate) semantic_completions: bool,
}
impl EvaluationBounds {
    /// Borrow the closed lower projection. Its failures alone prove nothing
    /// about the original partial schema and must not become diagnostics.
    #[cfg(test)]
    pub fn lower_program(&self) -> &EvaluationProgram {
        &self.lower
    }
    /// Borrow conservative refusal evidence at an original missing `$ref`.
    /// `None` means both projections are exact (there are no holes).
    #[cfg(test)]
    pub fn unavailable(&self) -> Option<&NoVerdict> {
        self.unavailable.as_ref()
    }
}

pub(super) fn space(request: &openbindings::SchemaRequest) -> (SchemaSpace, usize) {
    let space = SchemaSpace::new(
        Document::new(request.document().as_document().value().clone()),
        embedding::Supplied::new(request.supplied_resources().iter().map(|r| SchemaResource {
            uri: r.uri.clone(),
            document: r.document.clone(),
        }))
        .expect("admitted resources"),
    );
    let entry = space
        .document_node(request.entry())
        .expect("validated entry is a schema");
    (space, entry)
}
#[cfg(test)]
mod location_budget_tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn original_mapping_admits_complete_locations_and_decoding_separately() {
        let uri = "https://sdk-program.openbindings.invalid/p#/$defs/n0";
        let location = SchemaLocation {
            resource: Some("https://example.test/".to_owned() + &"u".repeat(32768)),
            pointer: "/é~0~1\n".parse().unwrap(),
        };
        let program = EvaluationProgram {
            entry_uri: uri.into(),
            resources: vec![],
            locations: Arc::new(BTreeMap::from([(
                uri.into(),
                ProgramOrigin::Prefix(location.clone()),
            )])),
        };
        let generated = uri.to_owned() + "/type";
        let required = location.resource.as_ref().unwrap().len()
            + openbindings_internal_json::backend::pointer_bytes(&location.pointer)
            + 5;
        for bytes in [0, 1, required - 1] {
            assert_eq!(
                program.original_location_bounded(&generated, bytes),
                Err(LocationBudgetExceeded)
            );
        }
        for bytes in [required, required + 1] {
            assert_eq!(
                program
                    .original_location_bounded(&generated, bytes)
                    .unwrap(),
                Some(SchemaLocation {
                    resource: location.resource.clone(),
                    pointer: openbindings_internal_json::backend::joined_pointer(
                        &location.pointer,
                        "/type"
                    )
                })
            );
        }
        assert_eq!(
            program.original_location(&generated),
            program
                .original_location_bounded(&generated, usize::MAX)
                .unwrap()
        );
        assert_eq!(
            program
                .original_location_bounded(&(uri.to_owned() + "/%74ype"), required)
                .unwrap(),
            program.original_location(&generated)
        );
        assert_eq!(
            program
                .original_location_bounded(&(uri.to_owned() + "/%GG"), required)
                .unwrap(),
            None
        );
        assert_eq!(
            program
                .original_location_bounded("https://unmapped.test/#/x", 0)
                .unwrap(),
            None
        );
    }
    #[test]
    fn percent_bearing_base_lookup_preserves_original_mapping_contract() {
        let at = SchemaLocation {
            resource: None,
            pointer: "".parse().unwrap(),
        };
        for base in [
            "https://example.test/%61",
            "https://example.test/%61#/$defs/n0",
        ] {
            let program = EvaluationProgram {
                entry_uri: base.into(),
                resources: vec![],
                locations: Arc::new(BTreeMap::from([(
                    base.into(),
                    ProgramOrigin::Prefix(at.clone()),
                )])),
            };
            assert_eq!(program.original_location(base), Some(at.clone()));
            assert_eq!(
                program.original_location_bounded(base, 0),
                Ok(Some(at.clone()))
            );
            assert_eq!(
                program.original_location_bounded(&(base.to_owned() + "/type"), 5),
                Ok(Some(SchemaLocation {
                    resource: None,
                    pointer: "/type".parse().unwrap()
                }))
            );
        }
        let uri = "https://example.test/%61#/%74ype";
        let program = EvaluationProgram {
            entry_uri: uri.into(),
            resources: vec![],
            locations: Arc::new(BTreeMap::from([(
                "https://example.test/%61#/type".into(),
                ProgramOrigin::Prefix(at.clone()),
            )])),
        };
        assert_eq!(
            program.original_location_bounded(uri, uri.len() - 1),
            Err(LocationBudgetExceeded)
        );
        assert_eq!(
            program.original_location_bounded(uri, uri.len()),
            Ok(Some(at))
        );
    }
}

#[cfg(test)]
trait BoundsRequest {
    fn evaluation_bounds(
        &self,
        control: &dyn openbindings::WorkControl,
    ) -> Result<EvaluationBounds, openbindings::NoVerdict>;
}
#[cfg(test)]
impl BoundsRequest for openbindings::SchemaRequest {
    fn evaluation_bounds(
        &self,
        control: &dyn openbindings::WorkControl,
    ) -> Result<EvaluationBounds, openbindings::NoVerdict> {
        let (space, entry) = space(self);
        space
            .bounds(entry, &|| control.is_cancelled())
            .map_err(crate::projection_failure)
    }
}
#[cfg(test)]
#[path = "../../tests/private/oneof_resources.rs"]
mod oneof_resources;
#[cfg(test)]
#[path = "../../tests/private/partial_resources.rs"]
mod partial_resources;
#[cfg(test)]
#[path = "../../tests/private/planner_locations.rs"]
mod planner_locations;
#[cfg(test)]
#[path = "../../tests/private/preparation_recovery.rs"]
mod preparation_recovery;
#[cfg(test)]
#[path = "../../tests/private/resource_identity.rs"]
mod resource_identity;
