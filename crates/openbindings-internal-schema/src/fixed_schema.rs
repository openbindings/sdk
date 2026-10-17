//! Fixed normative schema checks, compiled once without external acquisition.
use crate::{JsonRef, JsonValue};
use jsonschema::{Draft, Retrieve, Validator};
use openbindings_internal_json::{
    backend::{self, FlatJson, location_pointer_size, view},
    numeric,
};
use serde_json::Value;
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    sync::{Arc, OnceLock},
};

#[derive(Clone)]
pub(super) struct LocalResources(Arc<BTreeMap<String, Value>>);
impl Retrieve for LocalResources {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        self.0
            .get(uri.as_str())
            .cloned()
            .ok_or_else(|| format!("fixed resource unavailable: {uri}").into())
    }
}
#[derive(Debug)]
pub struct Problems<'a> {
    pub entries: Vec<Problem<'a>>,
    pub violated: bool,
    pub truncated: bool,
}

pub struct Problem<'a> {
    pub at: JsonRef<'a>,
    pub message: Cow<'static, str>,
}
impl std::fmt::Debug for Problem<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Problem")
            .field("at", &self.at)
            .finish_non_exhaustive()
    }
}
const GENERIC_MESSAGE: &str = "value violates the fixed normative schema";
// Source pointer retention shares the document findings allowance.
const MAX_EXPANDED_POINTER_BYTES: usize = 8 * 1024 * 1024;
type Compiled = Result<Validator<FlatJson>, String>;
static DOCUMENT: OnceLock<Compiled> = OnceLock::new();
static META: OnceLock<Compiled> = OnceLock::new();

pub(super) fn options(
    resources: BTreeMap<String, Value>,
) -> jsonschema::ValidationOptions<'static, Arc<dyn Retrieve>, FlatJson> {
    numeric::apply(jsonschema::options_for::<FlatJson>())
        .with_draft(Draft::Draft202012)
        .with_retriever(LocalResources(Arc::new(resources)))
        .should_validate_formats(false)
        // A check's allowance bounds all of its pattern work, so one long string
        // takes what it needs instead of failing an engine default per match.
        .with_pattern_options(jsonschema::PatternOptions::fancy_regex().backtrack_limit(usize::MAX))
}
pub(super) fn compile(schema: &Value, resources: BTreeMap<String, Value>) -> Compiled {
    options(resources).build(schema).map_err(|e| e.to_string())
}
/// Compile the pinned location map separately for allocation qualification.
pub fn compile_locations() -> Result<super::fixed_locations::LocationMap, String> {
    let schema: Value = serde_json::from_str(include_str!("../schemas/openbindings.schema.json"))
        .map_err(|e| e.to_string())?;
    super::fixed_locations::LocationMap::compile(&schema)
}
fn locations() -> &'static Result<super::fixed_locations::LocationMap, String> {
    static LOCATIONS: OnceLock<Result<super::fixed_locations::LocationMap, String>> =
        OnceLock::new();
    LOCATIONS.get_or_init(compile_locations)
}
/// Evaluator and pattern steps one fixed check may spend.
#[derive(Clone, Copy)]
pub(super) struct Allowance {
    steps: usize,
    patterns: usize,
}
// Each evaluator step enters one schema position for one instance node. The
// derived schema has no recursive reference and no applicator that revisits an
// instance (anyOf tries each branch once), so one node meets each position of
// the unrolled schema at most once. It has 80; a test counts them.
const STEPS_PER_NODE: usize = 80;
// Every derived-schema pattern is anchored and unambiguous, so its backtracking
// match is linear in the string. The costliest, SemVer on a nearly valid version,
// takes about 20 steps per byte, under a third of this (a test probes each
// pattern's near misses); location guidance revalidates within the rest.
const PATTERN_STEPS_PER_BYTE: usize = 64;
// One schema object's meta-schema check, with recursive edges cut to type checks.
const META_ALLOWANCE: Allowance = Allowance {
    steps: 2_000_000,
    patterns: 2_000_000,
};
impl Allowance {
    /// Spend from the enclosing check's allowance; unbounded outside a check.
    pub(super) const ENCLOSING: Self = Self {
        steps: usize::MAX,
        patterns: usize::MAX,
    };
    /// The whole-document check of `document`. It grows with the admitted
    /// document, so whatever the admission limits admit reaches a verdict, and
    /// is linear in those limits: its node count and its source bytes.
    fn document(document: JsonRef<'_>) -> Self {
        Self {
            steps: STEPS_PER_NODE.saturating_mul(backend::node_count(document)),
            patterns: PATTERN_STEPS_PER_BYTE.saturating_mul(document.text().len()),
        }
    }
    fn of(value: JsonRef<'_>, is_meta: bool) -> Self {
        if is_meta {
            META_ALLOWANCE
        } else {
            Self::document(value)
        }
    }
}
// Nested scopes consume the enclosing check's remaining allowance. Never expose
// an is_valid result from an incomplete work or pattern scope as rule evidence.
pub(super) fn bounded_valid(
    validator: &Validator<FlatJson>,
    at: JsonRef<'_>,
    allowance: Allowance,
) -> Result<bool, String> {
    jsonschema::ob_work::bounded(allowance.steps, 1024, || {
        jsonschema::ob_ecma::top_level(allowance.patterns, || {
            validator.is_valid(view(&at.to_owned()))
        })
    })
    .map_err(|e| format!("fixed validation work limit: {e:?}"))?
    .map_err(|_| "fixed pattern work limit".into())
}
/// The authoritative check alone, also used to qualify diagnostic allocation.
pub fn validity(value: &JsonValue, is_meta: bool) -> Result<bool, String> {
    let validator = (if is_meta { meta() } else { document() })
        .as_ref()
        .map_err(Clone::clone)?;
    let allowance = Allowance::of(value.view(), is_meta);
    backend::fixed_strings(|| bounded_valid(validator, value.view(), allowance))
}
fn document() -> &'static Compiled {
    DOCUMENT.get_or_init(|| {
        let schema: Value =
            serde_json::from_str(include_str!("../schemas/openbindings.schema.json"))
                .map_err(|e| e.to_string())?;
        compile(&schema, BTreeMap::new())
    })
}
fn localize(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.get("$dynamicRef").and_then(Value::as_str) == Some("#meta") {
                map.remove("$dynamicRef");
                map.insert("type".into(), serde_json::json!(["object", "boolean"]));
            }
            for (key, value) in map {
                if key == "$id" {
                    if let Some(id) = value.as_str() {
                        *value = Value::String(id.replace(
                            "https://json-schema.org/draft/2020-12/",
                            "https://openbindings.invalid/internal-meta/",
                        ));
                    }
                } else {
                    localize(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                localize(value);
            }
        }
        _ => {}
    }
}
pub(crate) const META_FILES: [&str; 8] = [
    include_str!("../schemas/draft2020-12/schema.json"),
    include_str!("../schemas/draft2020-12/meta/core.json"),
    include_str!("../schemas/draft2020-12/meta/applicator.json"),
    include_str!("../schemas/draft2020-12/meta/unevaluated.json"),
    include_str!("../schemas/draft2020-12/meta/validation.json"),
    include_str!("../schemas/draft2020-12/meta/meta-data.json"),
    include_str!("../schemas/draft2020-12/meta/format-annotation.json"),
    include_str!("../schemas/draft2020-12/meta/content.json"),
];
fn meta() -> &'static Compiled {
    META.get_or_init(|| {
        // Each direct subschema is checked separately by the normative-position walk.
        // Substituting only recursive meta edges with the object/boolean type check
        // preserves all local keyword constraints and avoids repeated ancestor work.
        let mut resources = BTreeMap::new();
        for text in META_FILES {
            let mut schema: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
            localize(&mut schema);
            let id = schema["$id"]
                .as_str()
                .ok_or("embedded meta-schema has no identifier")?
                .to_owned();
            resources.insert(id, schema);
        }
        let root = resources
            .get("https://openbindings.invalid/internal-meta/schema")
            .ok_or("embedded root meta-schema is missing")?
            .clone();
        compile(&root, resources)
    })
}
// A token's source address identifies its original occurrence while the borrowed
// arenas are live. Equal text/pointers at distinct spans remain distinct. No
// pointer is dereferenced or exposed, and no source coordinates are materialized.
pub fn occurrence(value: JsonRef<'_>) -> usize {
    value.text().as_ptr() as usize
}

struct ProblemCollector<'a> {
    entries: Vec<Problem<'a>>,
    retained: HashMap<usize, Vec<usize>>,
    max_problems: usize,
    violated: bool,
    truncated: bool,
    pointer_bytes: usize,
    expansion_exhausted: bool,
}
impl<'a> ProblemCollector<'a> {
    fn new(max_problems: usize) -> Self {
        Self {
            entries: Vec::new(),
            retained: HashMap::new(),
            max_problems,
            violated: false,
            truncated: false,
            pointer_bytes: MAX_EXPANDED_POINTER_BYTES,
            expansion_exhausted: false,
        }
    }
    // False means a distinct problem exceeded the count allowance; no later
    // problem can be retained. Duplicates are checked first, even at exact capacity.
    fn offer(
        &mut self,
        at: JsonRef<'a>,
        message: Cow<'static, str>,
        expanded: bool,
        already_retained: &impl Fn(JsonRef<'a>, &str) -> bool,
    ) -> bool {
        self.violated = true;
        let identity = occurrence(at);
        if already_retained(at, &message)
            || self.retained.get(&identity).is_some_and(|indices| {
                indices
                    .iter()
                    .any(|&index| self.entries[index].message == message)
            })
        {
            return true;
        }
        if self.entries.len() >= self.max_problems {
            self.truncated = true;
            return false;
        }
        if expanded {
            if self.expansion_exhausted {
                self.truncated = true;
                return true;
            }
            let Some(size) = location_pointer_size(at, self.pointer_bytes) else {
                self.truncated = true;
                self.expansion_exhausted = true;
                return true;
            };
            self.pointer_bytes -= size;
        }
        self.retained
            .entry(identity)
            .or_default()
            .push(self.entries.len());
        self.entries.push(Problem { at, message });
        true
    }
    fn finish(self) -> Problems<'a> {
        Problems {
            entries: self.entries,
            violated: self.violated,
            truncated: self.truncated,
        }
    }
}

pub fn check(
    value: &JsonValue,
    is_meta: bool,
    max_problems: usize,
) -> Result<Problems<'_>, String> {
    check_with_retained(value, is_meta, max_problems, |_, _| false)
}
pub fn check_with_retained<'a>(
    value: &'a JsonValue,
    is_meta: bool,
    max_problems: usize,
    already_retained: impl Fn(JsonRef<'a>, &str) -> bool,
) -> Result<Problems<'a>, String> {
    let validator = (if is_meta { meta() } else { document() })
        .as_ref()
        .map_err(Clone::clone)?;
    // Compilation is schema-only and cached, outside the per-document budget.
    let map = if is_meta {
        None
    } else {
        locations().as_ref().ok()
    };
    let allowance = Allowance::of(value.view(), is_meta);
    backend::fixed_strings(|| {
        let mut collector = ProblemCollector::new(max_problems);
        let mut decision = Err("fixed validation did not complete".into());
        // Location guidance spends what the verdict leaves of one allowance.
        let _walk_status = jsonschema::ob_work::bounded(allowance.steps, 1024, || {
            let _pattern_status = jsonschema::ob_ecma::top_level(allowance.patterns, || {
                decision = bounded_valid(validator, value.view(), Allowance::ENCLOSING);
                if decision != Ok(false) {
                    return;
                }
                collector.violated = true;
                if let Some(map) = map {
                    let mut offered = false;
                    let limited = map.locate(value.view(), |at| {
                        offered = true;
                        collector.offer(at, GENERIC_MESSAGE.into(), true, &already_retained)
                            && !collector.expansion_exhausted
                    });
                    collector.truncated |= limited;
                    if offered {
                        return;
                    }
                }
                collector.offer(
                    value.view(),
                    GENERIC_MESSAGE.into(),
                    false,
                    &already_retained,
                );
            });
        });
        // A location budget stop cannot erase a failure already established by
        // validity. A failed validity scope still returns its original refusal.
        decision?;
        Ok(collector.finish())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    #[test]
    fn step_allowance_is_the_position_count_of_the_unrolled_document_schema() {
        // One instance node meets each position of the unrolled schema at most
        // once, so their count bounds the evaluator steps it costs. That needs
        // no recursive reference and no applicator that revisits an instance.
        use crate::schema_index::{SCHEMA_SELECTORS, SchemaSelector};
        fn positions(root: &Value, schema: &Value, refs: &mut Vec<String>) -> usize {
            let object = match schema {
                Value::Bool(_) => return 1,
                Value::Object(object) => object,
                _ => return 0,
            };
            for keyword in [
                "$dynamicRef",
                "$recursiveRef",
                "unevaluatedProperties",
                "unevaluatedItems",
            ] {
                assert!(!object.contains_key(keyword), "{keyword}");
            }
            let mut count = 1;
            if let Some(target) = object.get("$ref").and_then(Value::as_str) {
                assert!(!refs.iter().any(|r| r == target), "recursive {target}");
                refs.push(target.to_owned());
                count += positions(root, root.pointer(&target[1..]).unwrap(), refs);
                refs.pop();
            }
            for (keyword, selector) in SCHEMA_SELECTORS {
                // Definitions apply only through a reference.
                if matches!(*keyword, "$defs" | "definitions") {
                    continue;
                }
                count += match (selector, object.get(*keyword)) {
                    (SchemaSelector::Direct, Some(child)) => positions(root, child, refs),
                    (
                        SchemaSelector::Map | SchemaSelector::MapOrStringArray,
                        Some(Value::Object(map)),
                    ) => map.values().map(|child| positions(root, child, refs)).sum(),
                    (SchemaSelector::Array, Some(Value::Array(items))) => {
                        items.iter().map(|child| positions(root, child, refs)).sum()
                    }
                    _ => 0,
                };
            }
            count
        }
        let schema: Value =
            serde_json::from_str(include_str!("../schemas/openbindings.schema.json")).unwrap();
        assert_eq!(positions(&schema, &schema, &mut Vec::new()), STEPS_PER_NODE);
    }

    #[test]
    fn near_misses_of_every_derived_pattern_need_under_a_third_of_the_allowance() {
        // A near miss forces the most backtracking. Validity's share leaves the
        // rest of the allowance to location guidance.
        let version =
            |body: &str| format!(r#"{{"openbindings":"0.0.0{body}!","operations":{{}}}}"#);
        let n = 10_000;
        let mut near_misses: Vec<String> = ["1", "1a", "-", "00a", "0a-1.", "1.", "a."]
            .iter()
            .map(|unit| version(&format!("-{}", unit.repeat(n))))
            .collect();
        near_misses.push(version(&format!("+{}", "a.".repeat(n))));
        near_misses.push(format!(
            "{{\"openbindings\":\"{}!.0.0\",\"operations\":{{}}}}",
            "1".repeat(n)
        ));
        for name in [
            "a".repeat(n) + "!",
            "a.-".repeat(n) + "!",
            "!".to_owned() + &"a".repeat(n),
        ] {
            near_misses.push(format!(
                r#"{{"openbindings":"0.2.0","operations":{{"{name}":{{}}}}}}"#
            ));
            near_misses.push(format!(
                r#"{{"openbindings":"0.2.0","operations":{{"o":{{"aliases":["{name}"]}}}}}}"#
            ));
        }
        for text in &near_misses {
            let value = JsonValue::parse(text).unwrap();
            let third = PATTERN_STEPS_PER_BYTE / 3 * text.len();
            let decided = jsonschema::ob_ecma::top_level(third, || validity(&value, false));
            assert!(matches!(decided, Ok(Ok(false))), "{}", &text[..48]);
        }
    }

    #[test]
    fn location_work_limit_preserves_established_failure_and_scope_recovers() {
        let value = JsonValue::parse(r#"{"openbindings":"0.2.0","name":0}"#).unwrap();
        let healthy = JsonValue::parse(r#"{"openbindings":"0.2.0","operations":{}}"#).unwrap();
        let mut saw_limited_failure = false;
        for work in 1..100 {
            let mut result = None;
            let _ = jsonschema::ob_work::bounded(work, 1024, || {
                result = Some(check(&value, false, 100));
            });
            if let Some(Ok(problems)) = result {
                assert!(problems.violated);
                assert!(!problems.entries.is_empty());
                assert!(
                    problems
                        .entries
                        .iter()
                        .all(|p| p.message == GENERIC_MESSAGE)
                );
                saw_limited_failure |= problems.truncated;
            }
            assert!(validity(&healthy, false).unwrap());
        }
        assert!(saw_limited_failure);
    }

    #[test]
    fn independent_container_and_child_findings_obey_the_same_cap() {
        let value = JsonValue::parse(r#"{"openbindings":"0.2.0","name":42}"#).unwrap();
        for cap in 0..=3 {
            let problems = check(&value, false, cap).unwrap();
            assert!(problems.violated);
            assert_eq!(problems.entries.len(), cap.min(2));
            assert_eq!(problems.truncated, cap < 2);
        }
    }

    #[test]
    fn retained_problem_identity_precedes_both_allowances() {
        let value = JsonValue::parse(r#"{"a":null,"b":null}"#).unwrap();
        let a = value.get("a").unwrap();
        let b = value.get("b").unwrap();
        let mut collector = ProblemCollector::new(1);
        collector.pointer_bytes = 2;
        assert!(collector.offer(a, "first".into(), true, &|_, _| false));
        assert_eq!(collector.pointer_bytes, 0);
        for _ in 0..10 {
            assert!(collector.offer(a, "first".into(), true, &|_, _| false));
        }
        assert!(!collector.truncated);
        assert_eq!(collector.retained.len(), 1);
        assert!(!collector.offer(b, "first".into(), true, &|_, _| false));
        assert!(collector.truncated);
        assert_eq!(collector.entries.len(), 1);
        assert_eq!(collector.retained.len(), 1);

        let mut collector = ProblemCollector::new(0);
        assert!(collector.offer(a, "existing".into(), true, &|at, message| {
            occurrence(at) == occurrence(a) && message == "existing"
        }));
        assert!(collector.violated);
        assert!(!collector.truncated);
        assert!(collector.retained.is_empty());
    }

    #[test]
    fn same_source_with_different_message_and_distinct_source_are_retained() {
        let value = JsonValue::parse(r#"[null,null]"#).unwrap();
        let a = value.view().element(0).unwrap();
        let b = value.view().element(1).unwrap();
        assert_eq!(a.text(), b.text());
        assert_ne!(occurrence(a), occurrence(b));
        let mut collector = ProblemCollector::new(3);
        for (at, message) in [(a, "first"), (a, "second"), (b, "first"), (a, "first")] {
            assert!(collector.offer(at, message.into(), false, &|_, _| false));
        }
        assert_eq!(collector.entries.len(), 3);
        assert!(!collector.truncated);
    }

    #[test]
    fn repeated_meta_null_error_does_not_consume_capacity_or_claim_truncation() {
        let value = JsonValue::parse("null").unwrap();
        let all = check(&value, true, 100).unwrap();
        let bounded = check(&value, true, all.entries.len()).unwrap();
        assert!(bounded.violated);
        assert!(!bounded.truncated);
        assert_eq!(bounded.entries.len(), all.entries.len());
        let identities: HashSet<_> = all
            .entries
            .iter()
            .map(|p| (occurrence(p.at), p.message.as_ref()))
            .collect();
        assert_eq!(identities.len(), all.entries.len());
        let already = check_with_retained(&value, true, 0, |at, message| {
            all.entries
                .iter()
                .any(|p| occurrence(p.at) == occurrence(at) && p.message == message)
        })
        .unwrap();
        assert!(already.violated);
        assert!(!already.truncated);
        assert!(already.entries.is_empty());
    }

    #[test]
    fn fixed_problem_count_paths_and_truncation_are_preserved() {
        let value = JsonValue::parse(
            r#"{"openbindings":"0.2.0","operations":{"a":{"description":1,"deprecated":2,"aliases":3}}}"#,
        ).unwrap();
        let all = check(&value, false, 100).unwrap();
        assert_eq!(all.entries.len(), 3);
        assert!(!all.truncated);
        for capacity in [0, 1, 2, 3] {
            let bounded = check(&value, false, capacity).unwrap();
            let retained = capacity.min(3);
            assert_eq!(bounded.entries.len(), retained);
            assert!(bounded.violated);
            assert_eq!(bounded.truncated, capacity < 3);
            for (actual, expected) in bounded.entries.iter().zip(&all.entries) {
                assert_eq!(actual.at.location(), expected.at.location());
                assert_eq!(actual.message, expected.message);
            }
        }
    }

    #[test]
    fn expanded_problems_preserve_violation_when_no_slots_remain() {
        let value =
            JsonValue::parse(r#"{"openbindings":"0.2.0","operations":{},"z":0,"a":0}"#).unwrap();
        for capacity in [0, 1, 2, 3] {
            let problems = check(&value, false, capacity).unwrap();
            assert!(problems.violated);
            assert_eq!(problems.entries.len(), capacity.min(2));
            assert_eq!(problems.truncated, capacity < 2);
            let paths: Vec<_> = problems
                .entries
                .iter()
                .map(|p| p.at.location().pointer.unwrap())
                .collect();
            assert_eq!(paths, ["/z", "/a"][..capacity.min(2)]);
        }
        let healthy = JsonValue::parse(r#"{"openbindings":"0.2.0","operations":{}}"#).unwrap();
        let problems = check(&healthy, false, 0).unwrap();
        assert!(!problems.violated);
        assert!(!problems.truncated);
        assert!(problems.entries.is_empty());
    }

    #[test]
    fn localized_meta_matches_unmodified_meta_on_keyword_shapes() {
        let resources: BTreeMap<_, _> = META_FILES
            .iter()
            .map(|text| {
                let v: Value = serde_json::from_str(text).unwrap();
                (v["$id"].as_str().unwrap().to_owned(), v)
            })
            .collect();
        let original =
            compile(&resources[crate::schema_index::DIALECT], resources.clone()).unwrap();
        let keywords = [
            "$schema",
            "$id",
            "$ref",
            "$dynamicRef",
            "$anchor",
            "$dynamicAnchor",
            "$defs",
            "type",
            "enum",
            "const",
            "required",
            "dependentRequired",
            "properties",
            "patternProperties",
            "additionalProperties",
            "dependentSchemas",
            "items",
            "prefixItems",
            "contains",
            "unevaluatedItems",
            "unevaluatedProperties",
            "allOf",
            "anyOf",
            "oneOf",
            "not",
            "if",
            "then",
            "else",
            "contentSchema",
            "definitions",
            "dependencies",
            "minimum",
            "exclusiveMinimum",
            "multipleOf",
            "maxLength",
            "minLength",
            "minItems",
            "uniqueItems",
            "pattern",
            "format",
            "default",
            "examples",
            "unknown",
        ];
        let values = [
            "null",
            "true",
            "false",
            "0",
            "-1",
            "1.0",
            r#""string""#,
            r#""[""#,
            "[]",
            r#"["a","a"]"#,
            "{}",
            r#"{"x":{"type":"invalid"}}"#,
            r#"[{"type":"invalid"}]"#,
            r#"{"$dynamicRef":{"type":"string"}}"#,
        ];
        for keyword in keywords {
            for text in values {
                let raw = format!("{{{}:{text}}}", serde_json::to_string(keyword).unwrap());
                let value = JsonValue::parse(&raw).unwrap();
                let want = original.is_valid(view(&value));
                let mut stack = vec![value.clone()];
                let mut actual = true;
                while let Some(node) = stack.pop() {
                    actual &= check(&node, true, 100).unwrap().entries.is_empty();
                    stack.extend(
                        crate::schema_index::schema_children(node.view())
                            .into_iter()
                            .map(|v| v.to_owned()),
                    );
                }
                assert_eq!(actual, want, "{raw}");
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/support/meta_equivalence.rs"]
mod equivalence;
