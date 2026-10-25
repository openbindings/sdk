//! Stand-ins for schema resources a request does not supply.
//!
//! The evaluator never fetches a reference. When a value contract reaches a resource
//! the request does not supply, preparation adds an empty resource at that URI
//! for each one and projects the value contract again, and every schema projected
//! from such a stand-in is compiled as the SDK's `$obMissing` keyword. A pass
//! that reaches it gives no verdict naming the missing resource; a value whose
//! evaluation never reaches it keeps the library's verdict. That verdict
//! cannot depend on the missing content where every keyword decides by
//! evaluating the subschemas it depends on, which a stand-in then answers.
//!
//! A reference to a fragment of a missing resource has no stand-in: the
//! second projection fails, and preparation refuses as the first did.
//!
//! An `unevaluatedProperties` or `unevaluatedItems` keyword decides from the
//! annotations of the schemas that apply to the same instance, which the
//! library reads from their structure rather than by evaluating them, so it
//! would read a stand-in as annotating nothing and decide without reaching
//! it. A value contract in which such a keyword's annotation sources can reach a
//! stand-in therefore refuses preparation, whatever order the library
//! evaluates keywords in. The same holds for the stand-in for an in-place
//! cycle and for `patternProperties` names the evaluator does not match.
use crate::bundle::Bundle;
use openbindings::JsonValue;
use openbindings_internal_schema::{ProjectedResource, SchemaSpace};
use std::collections::BTreeSet;

/// The SDK keyword a stand-in's schemas compile to; its value is the missing
/// resource's index.
pub(crate) const KEYWORD: &str = "$obMissing";

/// The resources the references in `space` name but no resource holds, in
/// order. Only the document's and the supplied resources' references can name
/// one: the packaged meta-schemas' name packaged resources.
pub(crate) fn uris(space: &SchemaSpace) -> Vec<String> {
    let mut missing = BTreeSet::new();
    for (id, node) in space.local_nodes() {
        for keyword in ["$ref", "$dynamicRef"] {
            if let Some(reference) = node.value.get(keyword).and_then(|value| value.as_str()) {
                let _ = space.resolve_with_missing(id, reference, &mut |uri| {
                    missing.insert(uri.to_owned());
                });
            }
        }
    }
    missing.into_iter().collect()
}

/// An empty resource for each of `uris`.
pub(crate) fn stand_ins(uris: &[String]) -> impl Iterator<Item = ProjectedResource> + '_ {
    uris.iter().map(|uri| ProjectedResource {
        uri: uri.clone(),
        document: JsonValue::parse("{}").expect("an empty object"),
    })
}

/// The stand-in, by the index its `keyword` holds, that an
/// `unevaluatedProperties` or `unevaluatedItems` keyword's annotation sources
/// reach, if any: every schema reached from the keyword's own schema through
/// the in-place applicators the library's walk follows (`$ref`, `$dynamicRef`,
/// `allOf`, `anyOf`, `oneOf`, `if`, `then`, `else` and `dependentSchemas`).
pub(crate) fn behind_unevaluated(bundle: &Bundle, keyword: &str) -> Option<u32> {
    let stand_in = |index: usize| {
        bundle.schemas[index]
            .get(keyword)
            .and_then(serde_json::Value::as_u64)
            .and_then(|at| u32::try_from(at).ok())
    };
    let mut seen = vec![false; bundle.schemas.len()];
    let mut pending: Vec<usize> = (0..bundle.schemas.len())
        .filter(|&index| {
            let schema = &bundle.schemas[index];
            schema.get("unevaluatedProperties").is_some()
                || schema.get("unevaluatedItems").is_some()
        })
        .collect();
    for &holder in &pending {
        seen[holder] = true;
    }
    while let Some(index) = pending.pop() {
        if let Some(at) = stand_in(index) {
            return Some(at);
        }
        for edge in &bundle.definitions[index].edges {
            let target = edge.to as usize;
            if edge.walked && !edge.descends && !seen[target] {
                seen[target] = true;
                pending.push(target);
            }
        }
    }
    None
}
