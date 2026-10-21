//! The unevaluated-walk transform: a stopgap for an upstream bug in
//! jsonschema 0.58.6.
//!
//! To build an `unevaluatedProperties` keyword, the library walks the schemas
//! that can evaluate the instance's members. For each `patternProperties` it
//! reaches, it compiles every subschema and then drops the compiled node
//! (`compile_pattern_properties` in `keywords/unevaluated_properties.rs`). A
//! `$ref` target compiled there can be one that a later compilation round
//! still points at; validation then reaches a dropped target and panics with
//! "pending schema target dropped", which aborts a WebAssembly host.
//!
//! The transform keeps every subschema out of that walk. In each definition
//! the walk can reach, `patternProperties` keeps its patterns, each with the
//! schema `true`, and the original subschemas apply from a definition of their
//! own, `{"not": {"not": {"patternProperties": ...}}}`, which the definition's
//! `allOf` references. The walk follows `allOf` and `$ref` but never enters
//! `not`, so it compiles only `true`; the subschemas are compiled once, owned
//! by their `not`. Validity is unchanged (`not` twice is the schema itself),
//! and so are the evaluated member names, which the walk still reads from the
//! patterns left in place. The added definition's name marks it, so an
//! explanation names the `patternProperties` it stands for. Each walk compiles
//! the `allOf` items it meets at its own location, so the subschemas sit
//! behind a reference, which every walk shares, rather than in the item.
//!
//! The transform depends on which keywords the library's walk follows.
//! `tests/unevaluated_walk.rs` reproduces the bug on a bundled program without
//! the transform and checks that the walk still skips `not`, so a library
//! upgrade that changes either fails those tests.
use crate::bundle::{self, Bundle, Definition};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Apply the transform to every definition an `unevaluatedProperties` walk
/// can reach.
pub(crate) fn apply(bundle: &mut Bundle) {
    let Bundle {
        names,
        schemas,
        definitions,
        ..
    } = bundle;
    // The walk starts at each holder and follows its walked references.
    let mut reached = vec![false; definitions.len()];
    let mut queue: Vec<usize> = (0..schemas.len())
        .filter(|&index| schemas[index].get("unevaluatedProperties").is_some())
        .collect();
    if queue.is_empty() {
        return;
    }
    for &holder in &queue {
        reached[holder] = true;
    }
    while let Some(index) = queue.pop() {
        for edge in &definitions[index].edges {
            let to = edge.to as usize;
            if edge.walked && !edge.descends && !reached[to] {
                reached[to] = true;
                queue.push(to);
            }
        }
    }
    // Move each reached definition's subschemas out, counting both sides'
    // references again; the moved subschemas reach only existing definitions.
    let mut moved = Vec::new();
    let mut indices: Option<HashMap<&str, u32>> = None;
    for index in (0..reached.len()).filter(|&index| reached[index]) {
        let Some(Value::Object(patterns)) = schemas[index].get_mut("patternProperties") else {
            continue;
        };
        if patterns.is_empty() {
            continue;
        }
        let original = std::mem::take(patterns);
        for pattern in original.keys() {
            patterns.insert(pattern.clone(), Value::Bool(true));
        }
        let mut held = Map::new();
        held.insert("patternProperties".into(), Value::Object(original));
        let mut held = serde_json::json!({ "not": { "not": Value::Object(held) } });
        let indices = indices.get_or_insert_with(|| {
            names
                .iter()
                .enumerate()
                .map(|(index, name)| (name.as_str(), index as u32))
                .collect()
        });
        let mut resolve = |reference: &str| {
            reference
                .strip_prefix("#/$defs/")
                .and_then(|name| indices.get(name))
                .copied()
        };
        let held_edges = bundle::edges(&mut held, &mut resolve);
        definitions[index].edges = bundle::edges(&mut schemas[index], &mut resolve);
        moved.push((index, held, held_edges));
    }
    drop(indices);
    for (holder, held, edges) in moved {
        let added = u32::try_from(names.len()).expect("bounded definitions");
        let number = definitions[holder].number;
        let name = bundle::added_name(number, added);
        let reference = serde_json::json!({ "$ref": format!("#/$defs/{name}") });
        match schemas[holder].as_object_mut().and_then(|schema| {
            schema
                .entry("allOf")
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
        }) {
            Some(all_of) => all_of.push(reference),
            None => unreachable!("a projected allOf is an array"),
        }
        definitions[holder]
            .edges
            .push(bundle::all_of_reference(added));
        names.push(name);
        schemas.push(held);
        definitions.push(Definition { number, edges });
    }
}
