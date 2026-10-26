//! Bounds programs: deciding a value whose pass reached a stand-in.
//!
//! A stand-in takes the place of what the evaluator cannot evaluate: a
//! resource that was not supplied, a schema that would apply again without
//! consuming the instance, or a pattern it does not match. A verdict pass
//! answers as if every stand-in it reaches held, and records the first; its
//! verdict then stands only if no answer of the stand-ins could change it.
//! The answer a stand-in would give can differ at every place evaluation
//! reaches it, at each instance and through each path, so the evaluator
//! decides only what holds however each such answer comes out.
//!
//! A bounds program computes that. It holds two copies of each schema that can
//! reach a stand-in, one for the least validity its answers can give the
//! schema (the lower bound) and one for the most (the upper bound), beside
//! every schema of the value contract's own program, and its root applies the
//! lower copy of the entry, the upper copy or the entry itself, as the pass's
//! phase (`crate::pass::Phase`) asks. A stand-in is
//! `false` in a lower copy and `true` in an upper one. A keyword whose validity
//! only grows with its subschemas' (`allOf`, `anyOf`, `$ref`, `properties`,
//! `items`, `contains` without `maxContains` and the rest) applies the same
//! bound of each, and `not` applies the other bound. `oneOf`, `if` and a
//! `contains` with `maxContains` grow with some outcomes of a subschema and
//! shrink with others, so their copies are written in other keywords from
//! both bounds of each subschema that can reach a stand-in:
//!
//! - `oneOf`: the lower bound holds when exactly one branch's upper bound holds
//!   and some branch's lower bound does; the upper bound when some branch's
//!   upper bound holds and at most one branch's lower bound does.
//! - `if`: where the condition's lower bound holds, the condition holds every
//!   way, so the `then` branch's bound applies; where its upper bound fails,
//!   the `else` branch's; where only the upper bound holds, the lower bound
//!   needs both branches' lower bounds and the upper bound either branch's
//!   upper bound.
//! - `contains`: the count of matching items lies between the items whose
//!   lower bound holds and those whose upper bound does, so the lower bound
//!   needs the first to reach `minContains` and the second to stay within
//!   `maxContains`, and the upper bound the reverse.
//!
//! A pattern stand-in (a `pattern` the evaluator does not match) fails every
//! string in a lower copy and holds in an upper one. A stand-in for
//! `patternProperties` names the evaluator does not match leaves each member
//! either matching each such name or not: the lower copy applies every such
//! name's subschema to every member, beside the schema's own
//! `additionalProperties`; the upper copy lets a member its other names and
//! properties leave out satisfy `additionalProperties` or any such subschema.
//!
//! By induction over the schemas, a lower copy holds only where its schema
//! holds every way the stand-ins' answers come out, independently at each
//! place, and an upper copy fails only where its schema fails every way: so
//! when the lower copy of the value contract holds, every way satisfies, and
//! when the upper copy fails, every way fails. A pass that reached a stand-in
//! and holds checks the lower copy; one that fails checks the upper. The
//! answers the pass gave are one of the ways, so the other bound cannot decide
//! it. A value contract whose entry applies nothing but stand-ins, through
//! `$ref` and `allOf`, has no bounds program: its copies are `false` and
//! `true`, which decide nothing.
//!
//! A copy refers to both copies of a subschema where its keyword is written
//! from both bounds, so nested `oneOf`, `if` and counted `contains` keywords
//! multiply the work of a pass over the copies. Every definition that can
//! reach a stand-in, copies and the value contract's own alike, is therefore
//! guarded by a work marker (`bundle::guard_definition`), and a pass over a
//! bound may do only a multiple of the work of the value contract's own entry,
//! which a pass over it in this program measures in the same units: past
//! that, a guard fails, the pass enters none of them more, and the value is
//! not decided. A schema that reaches no stand-in reads as the value
//! contract's own program writes it: no copy duplicates it, and a pass stops
//! reading the instance once its allowance is spent.
//!
//! An `unevaluatedProperties` or `unevaluatedItems` keyword reads annotations
//! of the schemas that apply to the same instance, and a schema's annotations
//! are kept only while it holds: a lower copy's are a subset of what any way
//! gives, and an upper copy's a superset, so the keyword's bound follows, while
//! its annotation sources are written in the same keywords as the value
//! contract's. Where a source's copy is written in other keywords (a source
//! holding one of the three above that can reach a stand-in), or a source is
//! another holder whose copies are so bounded, the holder's copies are `false`
//! and `true`, which bound any schema. Preparation already refuses a value
//! contract whose annotation sources reach a stand-in for a missing resource, a
//! cycle or names.
use crate::bundle::{self, Bundle, Definition};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// A bounds program, with the bundled indices of its entries: the lower and
/// upper copies of the value contract's entry and the entry itself.
pub(crate) struct Program {
    pub(crate) bundle: Bundle,
    pub(crate) lower: u32,
    pub(crate) upper: u32,
    pub(crate) original: u32,
}

/// The bounds program of `source`, a bundled value contract whose stand-in
/// patterns are the `pattern` matchers `stand_ins` marks, within `budget`
/// bytes of added definitions as [`bundle::definition_charge`] counts them;
/// `None` when its entry reaches no stand-in, applies nothing but stand-ins,
/// or the definitions would pass the budget.
pub(crate) fn program(source: &Bundle, stand_ins: &[bool], budget: usize) -> Option<Program> {
    if only_stand_ins(source) {
        return None;
    }
    let mut bounds = Bounds::new(source, stand_ins);
    if !bounds.tainted[source.entry as usize] {
        return None;
    }
    bounds.mark_unknown_holders();
    let lower = bounds.copy(source.entry, false);
    let upper = bounds.copy(source.entry, true);
    while let Some((index, upper)) = bounds.queue.pop() {
        let at = bounds.added[index].2;
        let schema = bounds.definition(at, upper, index);
        bounds.charge(&schema);
        bounds.added[index].3 = schema;
        if bounds.charged > budget {
            return None;
        }
    }
    Some(bounds.finish(lower, upper))
}

struct Bounds<'s> {
    source: &'s Bundle,
    // Each source definition's index, by name.
    index: HashMap<&'s str, u32>,
    // Whether each `pattern` matcher stands in.
    stand_ins: &'s [bool],
    // Whether each source definition can reach a stand-in.
    tainted: Vec<bool>,
    // Whether each source definition holds an unevaluated keyword whose
    // copies are `false` and `true`.
    unknown: Vec<bool>,
    // The definitions the program adds: name, projected number, the source
    // definition a copy copies, and schema.
    added: Vec<(String, u32, u32, Value)>,
    // Each copy's place in `added`, by source definition and bound.
    copies: HashMap<(u32, bool), usize>,
    // Copies whose schemas are still to be written, with their bound.
    queue: Vec<(usize, bool)>,
    // What the added definitions are charged.
    charged: usize,
}

// Whether a source schema is a stand-in definition: for a missing resource or
// for a schema that applies again.
fn stand_in_definition(schema: &Value) -> bool {
    schema.get(crate::missing::KEYWORD).is_some() || schema.get(crate::operands::CYCLE).is_some()
}
// Whether the value contract's entry applies to its instance nothing but
// stand-in definitions, through `$ref` and `allOf`, beside schemas that always
// hold: its lower copy is then `false` and its upper `true`, which decide
// nothing.
fn only_stand_ins(source: &Bundle) -> bool {
    // A schema that applies to its instance only the definitions it
    // references and schemas that always hold (`true` and work markers).
    fn references_only(value: &Value) -> bool {
        match value {
            Value::Bool(holds) => *holds,
            Value::Object(map) => map.iter().all(|(keyword, member)| match keyword.as_str() {
                "$ref" => member.is_string(),
                "allOf" => member
                    .as_array()
                    .is_some_and(|items| items.iter().all(references_only)),
                bundle::MARKER => true,
                _ => false,
            }),
            _ => false,
        }
    }
    let mut pending = vec![source.entry];
    let mut seen = vec![false; source.schemas.len()];
    let mut reached = false;
    while let Some(at) = pending.pop() {
        if std::mem::replace(&mut seen[at as usize], true) {
            continue;
        }
        let schema = &source.schemas[at as usize];
        if stand_in_definition(schema) {
            reached = true;
            continue;
        }
        if !references_only(schema) {
            return false;
        }
        pending.extend(
            source.definitions[at as usize]
                .edges
                .iter()
                .map(|edge| edge.to),
        );
    }
    reached
}
fn holds_unevaluated(schema: &Value) -> bool {
    schema.get("unevaluatedProperties").is_some() || schema.get("unevaluatedItems").is_some()
}

impl<'s> Bounds<'s> {
    fn new(source: &'s Bundle, stand_ins: &'s [bool]) -> Self {
        let count = source.schemas.len();
        let index = source
            .names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.as_str(), index as u32))
            .collect();
        let mut bounds = Bounds {
            source,
            index,
            stand_ins,
            tainted: vec![false; count],
            unknown: vec![false; count],
            added: Vec::new(),
            copies: HashMap::new(),
            queue: Vec::new(),
            charged: 0,
        };
        // A schema can reach a stand-in when it is one, holds one, or
        // references one that can.
        let mut referrers: Vec<Vec<u32>> = vec![Vec::new(); count];
        for (from, definition) in source.definitions.iter().enumerate() {
            for edge in &definition.edges {
                referrers[edge.to as usize].push(from as u32);
            }
        }
        let mut pending: Vec<u32> = (0..count as u32)
            .filter(|&at| bounds.holds_stand_in(&source.schemas[at as usize]))
            .collect();
        for &at in &pending {
            bounds.tainted[at as usize] = true;
        }
        while let Some(at) = pending.pop() {
            for &from in &referrers[at as usize] {
                if !bounds.tainted[from as usize] {
                    bounds.tainted[from as usize] = true;
                    pending.push(from);
                }
            }
        }
        bounds
    }
    fn charge(&mut self, schema: &Value) {
        self.charged = self
            .charged
            .saturating_add(bundle::definition_charge(schema));
    }
    // Whether a source definition is a stand-in or holds one itself.
    fn holds_stand_in(&self, schema: &Value) -> bool {
        stand_in_definition(schema)
            || schema.get(crate::operands::NAMES).is_some()
            || schema
                .get("pattern")
                .is_some_and(|pattern| self.stand_in_pattern(pattern))
    }
    // Whether a `pattern` keyword's value is a stand-in matcher's index.
    fn stand_in_pattern(&self, value: &Value) -> bool {
        value
            .as_str()
            .and_then(|index| index.parse::<usize>().ok())
            .is_some_and(|index| self.stand_ins.get(index).copied().unwrap_or(false))
    }
    // The source definition a bundled reference names.
    fn target(&self, reference: &str) -> Option<u32> {
        reference
            .strip_prefix("#/$defs/")
            .and_then(|name| self.index.get(name))
            .copied()
    }
    // Whether a subschema value can reach a stand-in.
    fn reaches(&self, value: &Value) -> bool {
        bundle::any_reference(value, &mut |reference| {
            self.target(reference)
                .is_some_and(|at| self.tainted[at as usize])
        })
    }
    // Which keywords of a schema object its copies write in other keywords:
    // a `oneOf` with a branch, an `if` with a branch and a condition, and a
    // `contains` with a `maxContains` and a subschema, that can reach a
    // stand-in.
    fn rewritten(&self, map: &Map<String, Value>) -> (bool, bool, bool) {
        let one_of = map
            .get("oneOf")
            .and_then(Value::as_array)
            .is_some_and(|branches| branches.iter().any(|branch| self.reaches(branch)));
        let conditional = (map.contains_key("then") || map.contains_key("else"))
            && map
                .get("if")
                .is_some_and(|condition| self.reaches(condition));
        let counted = map.contains_key("maxContains")
            && map.get("contains").is_some_and(|items| self.reaches(items));
        (one_of, conditional, counted)
    }
    // The holders of unevaluated keywords whose copies must be `false` and
    // `true`: those whose annotation sources (every schema the in-place
    // references the library's walk follows reach, the holder's own
    // included) hold a keyword their copies write in other keywords, or a
    // holder that is one of them.
    fn mark_unknown_holders(&mut self) {
        let source = self.source;
        let holders: Vec<u32> = (0..source.schemas.len() as u32)
            .filter(|&at| {
                self.tainted[at as usize] && holds_unevaluated(&source.schemas[at as usize])
            })
            .collect();
        if holders.is_empty() {
            return;
        }
        let mut sources: Vec<Vec<u32>> = Vec::with_capacity(holders.len());
        let mut seen = vec![false; source.schemas.len()];
        for &holder in &holders {
            let mut pending = vec![holder];
            seen[holder as usize] = true;
            let mut reached = Vec::new();
            while let Some(at) = pending.pop() {
                reached.push(at);
                for edge in &source.definitions[at as usize].edges {
                    if edge.walked && !edge.descends && !seen[edge.to as usize] {
                        seen[edge.to as usize] = true;
                        pending.push(edge.to);
                    }
                }
            }
            for &at in &reached {
                seen[at as usize] = false;
            }
            sources.push(reached);
        }
        for (&holder, reached) in holders.iter().zip(&sources) {
            if reached.iter().any(|&at| {
                self.tainted[at as usize]
                    && match &source.schemas[at as usize] {
                        Value::Object(map) => {
                            let (one_of, conditional, counted) = self.rewritten(map);
                            one_of || conditional || counted
                        }
                        _ => false,
                    }
            }) {
                self.unknown[holder as usize] = true;
            }
        }
        // A holder whose sources reach another that is unknown is unknown too.
        loop {
            let mut changed = false;
            for (&holder, reached) in holders.iter().zip(&sources) {
                if !self.unknown[holder as usize]
                    && reached
                        .iter()
                        .any(|&at| at != holder && self.unknown[at as usize])
                {
                    self.unknown[holder as usize] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
    // The definition a copy for `upper` references in place of source
    // definition `at`: its copy for that bound when it can reach a stand-in,
    // and the shared definition otherwise.
    fn copy(&mut self, at: u32, upper: bool) -> u32 {
        if !self.tainted[at as usize] {
            return at;
        }
        if let Some(&index) = self.copies.get(&(at, upper)) {
            return self.copied(index);
        }
        let name = format!(
            "{}-{}",
            self.source.names[at as usize],
            if upper { 'u' } else { 'l' }
        );
        let number = self.source.definitions[at as usize].number;
        let index = self.added.len();
        self.added.push((name, number, at, Value::Null));
        self.copies.insert((at, upper), index);
        self.queue.push((index, upper));
        self.copied(index)
    }
    // An added definition's index in the program, past every source index.
    fn copied(&self, index: usize) -> u32 {
        (self.source.schemas.len() + index) as u32
    }
    fn name(&self, index: u32) -> &str {
        let index = index as usize;
        match index.checked_sub(self.source.schemas.len()) {
            Some(added) => &self.added[added].0,
            None => &self.source.names[index],
        }
    }
    // A reference to definition `index` of the program.
    fn reference(&self, index: u32) -> Value {
        let mut object = Map::new();
        object.insert(
            "$ref".into(),
            Value::String(format!("#/$defs/{}", self.name(index))),
        );
        Value::Object(object)
    }
    // A subschema value written for `upper`.
    fn value(&mut self, value: &Value, upper: bool) -> Value {
        match value {
            Value::Object(map) => Value::Object(self.object(map, upper, None)),
            other => other.clone(),
        }
    }
    fn values(&mut self, values: &[Value], upper: bool) -> Vec<Value> {
        values
            .iter()
            .map(|value| self.value(value, upper))
            .collect()
    }
    // The schema of the copy at `index` in `added` of source definition `at`.
    fn definition(&mut self, at: u32, upper: bool, index: usize) -> Value {
        let source = self.source;
        let schema = &source.schemas[at as usize];
        if self.unknown[at as usize] || stand_in_definition(schema) {
            return Value::Bool(upper);
        }
        match schema {
            Value::Object(map) => Value::Object(self.object(map, upper, Some(index))),
            other => other.clone(),
        }
    }
    // Write `schema`, which stands for `keyword` of the copy at `copy` in
    // `added` when it is a definition's root, as a definition of its own, and
    // give the `allOf` items that apply it.
    fn stand(&mut self, copy: Option<usize>, keyword: &str, schema: Value, items: &mut Vec<Value>) {
        let Some(copy) = copy else {
            items.push(schema);
            return;
        };
        let name = bundle::standing_name(&self.added[copy].0, keyword);
        let number = self.added[copy].1;
        self.charge(&schema);
        let index = self.added.len();
        self.added.push((name, number, u32::MAX, schema));
        if self.source.markers {
            items.push(serde_json::json!({ bundle::MARKER: 0 }));
        }
        items.push(self.reference(self.copied(index)));
    }
    // A schema object written for `upper`; `copy` is the copy's place in
    // `added` when the object is its root.
    fn object(
        &mut self,
        map: &Map<String, Value>,
        upper: bool,
        copy: Option<usize>,
    ) -> Map<String, Value> {
        let mut out = Map::new();
        let mut items = Vec::new();
        let (one_of, conditional, counted) = self.rewritten(map);
        let names = copy.is_some() && map.contains_key(crate::operands::NAMES);
        for (keyword, value) in map {
            let written = match keyword.as_str() {
                "$ref" => match value.as_str().and_then(|reference| self.target(reference)) {
                    Some(at) => {
                        let index = self.copy(at, upper);
                        Value::String(format!("#/$defs/{}", self.name(index)))
                    }
                    None => value.clone(),
                },
                "not" => self.value(value, !upper),
                "oneOf" if one_of => continue,
                "if" | "then" | "else" if conditional => continue,
                "contains" | "minContains" | "maxContains" if counted => continue,
                "allOf" | "anyOf" | "oneOf" | "prefixItems" => match value {
                    Value::Array(items) => Value::Array(self.values(items, upper)),
                    other => other.clone(),
                },
                "properties" | "patternProperties" | "dependentSchemas" => match value {
                    Value::Object(members) => Value::Object(
                        members
                            .iter()
                            .map(|(name, member)| (name.clone(), self.value(member, upper)))
                            .collect(),
                    ),
                    other => other.clone(),
                },
                "additionalProperties"
                | "propertyNames"
                | "items"
                | "contains"
                | "unevaluatedItems"
                | "unevaluatedProperties"
                | "if"
                | "then"
                | "else" => self.value(value, upper),
                "pattern" if self.stand_in_pattern(value) => {
                    // The pattern fails every string in the lower copy.
                    if !upper {
                        items.push(serde_json::json!({ "not": { "type": "string" } }));
                    }
                    continue;
                }
                crate::operands::NAMES
                | crate::operands::NAME_SCHEMAS
                | crate::operands::NAME_MEMBER => continue,
                _ => value.clone(),
            };
            out.insert(keyword.clone(), written);
        }
        if names {
            let schemas: Vec<Value> = map
                .get(crate::operands::NAME_SCHEMAS)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if upper {
                // A member the schema's other names and properties leave out
                // satisfies `additionalProperties` or any such name's subschema.
                if let Some(additional) = map.get("additionalProperties") {
                    let mut either = vec![self.value(additional, true)];
                    either.extend(self.values(&schemas, true));
                    let either = serde_json::json!({ "anyOf": either });
                    let mut applied = Vec::new();
                    self.stand(copy, "additionalProperties", either, &mut applied);
                    let written = match applied.as_slice() {
                        [reference] => reference.clone(),
                        _ => serde_json::json!({ "allOf": applied }),
                    };
                    out.insert("additionalProperties".into(), written);
                }
            } else {
                // Every member satisfies every such name's subschema.
                let every = serde_json::json!({
                    "additionalProperties": { "allOf": self.values(&schemas, false) }
                });
                self.stand(copy, "patternProperties", every, &mut items);
            }
        }
        if one_of && let Some(Value::Array(branches)) = map.get("oneOf") {
            let lower = self.values(branches, false);
            let upper_branches = self.values(branches, true);
            let schema = if upper {
                serde_json::json!({ "allOf": [
                    { "anyOf": upper_branches },
                    { "anyOf": [{ "oneOf": lower }, { "not": { "anyOf": lower } }] },
                ] })
            } else {
                serde_json::json!({ "allOf": [
                    { "oneOf": upper_branches },
                    { "anyOf": lower },
                ] })
            };
            self.stand(copy, "oneOf", schema, &mut items);
        }
        if conditional && let Some(condition) = map.get("if") {
            let truth = Value::Bool(true);
            let then = map.get("then").unwrap_or(&truth);
            let otherwise = map.get("else").unwrap_or(&truth);
            let surely = self.value(condition, false);
            let possibly = self.value(condition, true);
            let then = self.value(then, upper);
            let otherwise = self.value(otherwise, upper);
            let both = if upper {
                serde_json::json!({ "anyOf": [then, otherwise] })
            } else {
                serde_json::json!({ "allOf": [then, otherwise] })
            };
            let schema = serde_json::json!({
                "if": surely,
                "then": then,
                "else": { "if": possibly, "then": both, "else": otherwise },
            });
            self.stand(copy, "if", schema, &mut items);
        }
        if counted && let Some(contained) = map.get("contains") {
            let (reaching, within) = if upper {
                (self.value(contained, true), self.value(contained, false))
            } else {
                (self.value(contained, false), self.value(contained, true))
            };
            let mut enough = Map::new();
            enough.insert("contains".into(), reaching);
            if let Some(least) = map.get("minContains") {
                enough.insert("minContains".into(), least.clone());
            }
            let most = map.get("maxContains").cloned().unwrap_or(Value::Null);
            let schema = serde_json::json!({ "allOf": [
                Value::Object(enough),
                { "contains": within, "minContains": 0, "maxContains": most },
            ] });
            self.stand(copy, "contains", schema, &mut items);
        }
        if !items.is_empty() {
            match out.get_mut("allOf") {
                Some(Value::Array(all_of)) => all_of.extend(items),
                _ => {
                    out.insert("allOf".into(), Value::Array(items));
                }
            }
        }
        out
    }
    // The program: every source definition, then every one it adds. Those
    // that can reach a stand-in, copies and the value contract's own alike,
    // are guarded by a work marker, which counts each entry and enters none
    // once a pass's work allowance is spent; every other definition reads as
    // the value contract's own program writes it.
    fn finish(mut self, lower: u32, upper: u32) -> Program {
        let source = self.source;
        let count = source.schemas.len();
        let mut names = source.names.clone();
        let mut schemas = source.schemas.clone();
        let mut numbers = Vec::with_capacity(self.added.len());
        for (name, number, _, schema) in std::mem::take(&mut self.added) {
            names.push(name);
            schemas.push(schema);
            numbers.push(number);
        }
        let indices: HashMap<&str, u32> = names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.as_str(), index as u32))
            .collect();
        let mut definitions = Vec::with_capacity(schemas.len());
        for (at, schema) in schemas.iter_mut().enumerate() {
            if at < count && !self.tainted[at] {
                definitions.push(Definition {
                    number: source.definitions[at].number,
                    edges: source.definitions[at].edges.clone(),
                });
                continue;
            }
            let number = match at.checked_sub(count) {
                Some(added) => numbers[added],
                None => source.definitions[at].number,
            };
            let mut resolve = |reference: &str| {
                reference
                    .strip_prefix("#/$defs/")
                    .and_then(|name| indices.get(name))
                    .copied()
            };
            bundle::guard_definition(schema);
            let edges = bundle::edges(schema, &mut resolve);
            definitions.push(Definition { number, edges });
        }
        drop(indices);
        Program {
            bundle: Bundle::assembled(names, schemas, definitions, source.entry, source.markers),
            lower,
            upper,
            original: source.entry,
        }
    }
}
