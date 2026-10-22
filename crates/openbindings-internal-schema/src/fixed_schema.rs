//! Fixed normative schema checks, compiled once without external acquisition.
//!
//! The checks run on the unmodified jsonschema library with its default pattern
//! engine. Every fixed pattern is regular (no backreference or lookaround), so
//! that engine hands each one whole to a finite-automaton matcher whose time is
//! linear in the string and whose memory is bounded by the pattern. The derived
//! document schema has no recursive reference, and the meta-schema check cuts
//! its recursive edges, so one instance node meets each schema position at most
//! once. A check's work is therefore linear in the admitted document and needs
//! no budget of its own: the admission limits bound it. Its memory is the
//! document's own arena plus a bounded matcher cache and the capped findings,
//! and its stack depth follows the fixed schemas, not the instance.
use crate::{JsonRef, JsonValue};
use jsonschema::json::{JsonNumber, Node};
use jsonschema::{Draft, Keyword, Retrieve, ValidationError, ValidationOptions, Validator};
use openbindings_internal_json::{
    backend::{self, FlatJson, View, location_pointer_size, view},
    numeric,
};
use serde_json::Value;
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, OnceLock, PoisonError},
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
// OBI-10 for a schema that is neither an object nor a boolean.
const SCHEMA_TYPE_MESSAGE: &str = "a schema must be a JSON object or boolean";
// Source pointer retention shares the document findings allowance.
const MAX_EXPANDED_POINTER_BYTES: usize = 8 * 1024 * 1024;
type Compiled = Result<Validator<FlatJson>, String>;
static DOCUMENT: OnceLock<Compiled> = OnceLock::new();
static META: OnceLock<Compiled> = OnceLock::new();

/// An exact comparison of a number with a fixed schema's own operand. The fixed
/// schemas assert only the four comparisons (a test checks every position), and
/// those always decide; their operands are small and exactly representable.
struct ExactComparison(numeric::Bound);
impl<'i> Keyword<'i, FlatJson> for ExactComparison {
    fn is_valid(&self, instance: View<'i>) -> bool {
        let Some(number) = instance.as_number() else {
            return true;
        };
        match self.0.admits(&number.as_str()) {
            Some(admitted) => admitted,
            None => unreachable!("only multipleOf can exceed the exact arithmetic"),
        }
    }
    fn validate(&self, instance: View<'i>) -> Result<(), ValidationError<'i>> {
        if <Self as Keyword<'i, FlatJson>>::is_valid(self, instance) {
            Ok(())
        } else {
            Err(ValidationError::custom(format!(
                "exact numeric {} constraint failed",
                self.0.keyword()
            )))
        }
    }
}
fn exact_comparisons<R>(
    mut options: ValidationOptions<'_, R, FlatJson>,
) -> ValidationOptions<'_, R, FlatJson> {
    for keyword in numeric::COMPARISONS {
        options = options.with_keyword(keyword, move |_, value, _| {
            let Some(number) = value.as_number() else {
                return Err(ValidationError::custom("numeric keyword requires a number"));
            };
            let bound = numeric::Bound::parse(keyword, &number.to_string())
                .map_err(ValidationError::custom)?;
            Ok(Box::new(ExactComparison(bound)))
        });
    }
    options
}
/// The library's own `pattern` keyword, compiled once for each distinct pattern
/// and shared by every fixed validator. The derived schema repeats the name
/// grammar at many positions, and the location plan compiles a validator for
/// every location, each of which would compile its own copy of every pattern
/// beneath it: about ninety copies of one pattern, each a few kilobytes of
/// compiled matcher. Each fixed check, and every location validator, consults
/// the one shared validator of `{"pattern": ...}` instead.
struct SharedPattern(Arc<Validator<FlatJson>>);
impl<'i> Keyword<'i, FlatJson> for SharedPattern {
    fn is_valid(&self, instance: View<'i>) -> bool {
        self.0.is_valid(instance)
    }
    fn validate(&self, instance: View<'i>) -> Result<(), ValidationError<'i>> {
        if <Self as Keyword<'i, FlatJson>>::is_valid(self, instance) {
            Ok(())
        } else {
            Err(ValidationError::custom("value does not match the pattern"))
        }
    }
}
fn shared_pattern(pattern: &Value) -> Result<SharedPattern, ValidationError<'static>> {
    // The fixed schemas hold five distinct patterns, so the table stays small.
    static PATTERNS: Mutex<BTreeMap<String, Arc<Validator<FlatJson>>>> =
        Mutex::new(BTreeMap::new());
    let text = pattern
        .as_str()
        .ok_or_else(|| ValidationError::custom("pattern requires a string"))?;
    let mut patterns = PATTERNS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(validator) = patterns.get(text) {
        return Ok(SharedPattern(validator.clone()));
    }
    let validator = Arc::new(
        jsonschema::options_for::<FlatJson>()
            .with_draft(Draft::Draft202012)
            .build(&serde_json::json!({ "pattern": text }))?,
    );
    patterns.insert(text.to_owned(), validator.clone());
    Ok(SharedPattern(validator))
}
/// The fixed checks' validation options: exact comparisons, no format
/// assertion, only the local resources given, and the library's default
/// pattern engine, each pattern compiled once.
pub(super) fn options(
    resources: BTreeMap<String, Value>,
) -> ValidationOptions<'static, Arc<dyn Retrieve>, FlatJson> {
    exact_comparisons(jsonschema::options_for::<FlatJson>())
        .with_keyword("pattern", |_, value, _| {
            Ok(Box::new(shared_pattern(value)?))
        })
        .with_draft(Draft::Draft202012)
        .with_retriever(LocalResources(Arc::new(resources)))
        .should_validate_formats(false)
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
/// One fixed validator's verdict on `at`, under the fixed-string presentation
/// the caller has entered.
pub(super) fn valid(validator: &Validator<FlatJson>, at: JsonRef<'_>) -> bool {
    validator.is_valid(view(&at.to_owned()))
}
/// The authoritative check alone, also used to qualify diagnostic allocation.
pub fn validity(value: &JsonValue, is_meta: bool) -> Result<bool, String> {
    let validator = (if is_meta { meta() } else { document() })
        .as_ref()
        .map_err(Clone::clone)?;
    Ok(backend::fixed_strings(|| valid(validator, value.view())))
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
// The localized meta-schema resources, by identifier. Each direct subschema is
// checked separately by the normative-position walk. Substituting only
// recursive meta edges with the object/boolean type check preserves all local
// keyword constraints and avoids repeated ancestor work.
fn meta_resources() -> Result<BTreeMap<String, Value>, String> {
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
    Ok(resources)
}
fn meta() -> &'static Compiled {
    META.get_or_init(|| {
        let resources = meta_resources()?;
        let root = resources
            .get("https://openbindings.invalid/internal-meta/schema")
            .ok_or("embedded root meta-schema is missing")?
            .clone();
        compile(&root, resources)
    })
}
/// Each meta-schema keyword's own check, which locates an OBI-10 failure at the
/// keywords whose values fail. Every localized meta-schema resource asserts only
/// that a schema is an object or a boolean and, through `properties`, the value
/// of each keyword it defines, so an object fails the meta-schemas exactly when
/// one of its keywords fails its own check. As with the location map, a process
/// whose schemas all conform never builds it; the first failure builds it once.
struct MetaKeywords {
    validators: jsonschema::ValidatorMap<FlatJson>,
    // By keyword: its validator's key and the message of a finding at its value.
    keywords: BTreeMap<String, (String, String)>,
}
fn meta_keywords() -> &'static Result<MetaKeywords, String> {
    static KEYWORDS: OnceLock<Result<MetaKeywords, String>> = OnceLock::new();
    KEYWORDS.get_or_init(|| {
        let resources = meta_resources()?;
        let mut definitions = serde_json::Map::new();
        let mut keywords = BTreeMap::new();
        for (id, schema) in &resources {
            let Some(Value::Object(properties)) = schema.get("properties") else {
                continue;
            };
            for keyword in properties.keys() {
                // Meta-schema keywords hold no `~` or `/`, so each names itself
                // as a pointer token.
                if keyword.contains(['~', '/']) || definitions.contains_key(keyword) {
                    return Err(format!("unsupported meta-schema keyword: {keyword}"));
                }
                definitions.insert(
                    keyword.clone(),
                    serde_json::json!({ "$ref": format!("{id}#/properties/{keyword}") }),
                );
                let message = format!(
                    "the value of the {keyword} keyword does not satisfy the JSON Schema 2020-12 meta-schema"
                );
                keywords.insert(keyword.clone(), (format!("#/$defs/{keyword}"), message));
            }
        }
        let validators = options(resources)
            .build_map(&serde_json::json!({ "$defs": definitions }))
            .map_err(|e| e.to_string())?;
        if keywords.values().any(|(key, _)| validators.get(key).is_none()) {
            return Err("unmapped meta-schema keyword".into());
        }
        Ok(MetaKeywords {
            validators,
            keywords,
        })
    })
}
// Offer an OBI-10 finding at each keyword of the failing schema `value` whose
// value fails its own check, or at the schema itself when it is not an object;
// return whether any was offered.
fn locate_meta<'a>(
    value: JsonRef<'a>,
    meta: &'static MetaKeywords,
    collector: &mut ProblemCollector<'a>,
    already_retained: &impl Fn(JsonRef<'a>, &str) -> bool,
) -> bool {
    let Some(members) = value.members() else {
        collector.offer(value, SCHEMA_TYPE_MESSAGE.into(), false, already_retained);
        return true;
    };
    let mut offered = false;
    for member in members {
        let Some((key, message)) = member
            .name
            .as_str()
            .and_then(|name| meta.keywords.get(name))
        else {
            continue;
        };
        let Some(validator) = meta.validators.get(key) else {
            continue;
        };
        if valid(validator, member.value) {
            continue;
        }
        offered = true;
        if !collector.offer(
            member.value,
            Cow::Borrowed(message.as_str()),
            true,
            already_retained,
        ) || collector.expansion_exhausted
        {
            break;
        }
    }
    offered
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
    check_with_retained(value.view(), is_meta, max_problems, |_, _| false)
}
pub fn check_with_retained<'a>(
    value: JsonRef<'a>,
    is_meta: bool,
    max_problems: usize,
    already_retained: impl Fn(JsonRef<'a>, &str) -> bool,
) -> Result<Problems<'a>, String> {
    let validator = (if is_meta { meta() } else { document() })
        .as_ref()
        .map_err(Clone::clone)?;
    Ok(backend::fixed_strings(|| {
        let mut collector = ProblemCollector::new(max_problems);
        if valid(validator, value) {
            return collector.finish();
        }
        collector.violated = true;
        // Only a failing check needs guidance. A schema's guidance checks each
        // of its keywords on its own; a document's plan compiles a validator for
        // every location of the derived schema. A process whose schemas or
        // documents all conform never builds either; the first failure builds
        // it once.
        if is_meta {
            if let Ok(meta) = meta_keywords()
                && locate_meta(value, meta, &mut collector, &already_retained)
            {
                return collector.finish();
            }
            collector.offer(value, GENERIC_MESSAGE.into(), false, &already_retained);
            return collector.finish();
        }
        if let Ok(map) = locations().as_ref() {
            let mut offered = false;
            let limited = map.locate(value, |at| {
                offered = true;
                collector.offer(at, GENERIC_MESSAGE.into(), true, &already_retained)
                    && !collector.expansion_exhausted
            });
            collector.truncated |= limited;
            if offered {
                return collector.finish();
            }
        }
        collector.offer(value, GENERIC_MESSAGE.into(), false, &already_retained);
        collector.finish()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    #[test]
    fn document_schema_meets_each_node_at_a_bounded_number_of_positions() {
        // One instance node meets each position of the unrolled schema at most
        // once, so their count bounds the validator's work per node, and the
        // unrolled depth bounds its stack. That needs no recursive reference and
        // no applicator that revisits an instance (anyOf tries each branch once).
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
        assert_eq!(positions(&schema, &schema, &mut Vec::new()), 80);
    }

    #[test]
    fn cut_meta_schema_meets_each_node_at_a_bounded_number_of_positions() {
        // Cutting the recursive edges leaves a finite tree of positions across the
        // localized meta-schema resources; one node meets each at most once.
        use crate::schema_index::{SCHEMA_SELECTORS, SchemaSelector};
        let resources: BTreeMap<String, Value> = META_FILES
            .iter()
            .map(|text| {
                let mut schema: Value = serde_json::from_str(text).unwrap();
                localize(&mut schema);
                (schema["$id"].as_str().unwrap().to_owned(), schema)
            })
            .collect();
        fn positions(
            resources: &BTreeMap<String, Value>,
            base: &str,
            schema: &Value,
            refs: &mut Vec<String>,
        ) -> usize {
            let object = match schema {
                Value::Bool(_) => return 1,
                Value::Object(object) => object,
                _ => return 0,
            };
            for keyword in ["$dynamicRef", "$recursiveRef"] {
                assert!(!object.contains_key(keyword), "{keyword}");
            }
            let mut count = 1;
            if let Some(target) = object.get("$ref").and_then(Value::as_str) {
                let (path, fragment) = target.split_once('#').unwrap_or((target, ""));
                let uri = if path.is_empty() {
                    base.to_owned()
                } else {
                    format!("{}{path}", &base[..=base.rfind('/').unwrap()])
                };
                let absolute = format!("{uri}#{fragment}");
                assert!(!refs.contains(&absolute), "recursive {absolute}");
                refs.push(absolute);
                let resolved = resources[&uri].pointer(fragment).unwrap();
                count += positions(resources, &uri, resolved, refs);
                refs.pop();
            }
            for (keyword, selector) in SCHEMA_SELECTORS {
                if matches!(*keyword, "$defs" | "definitions") {
                    continue;
                }
                count += match (selector, object.get(*keyword)) {
                    (SchemaSelector::Direct, Some(child)) => {
                        positions(resources, base, child, refs)
                    }
                    (
                        SchemaSelector::Map | SchemaSelector::MapOrStringArray,
                        Some(Value::Object(map)),
                    ) => map
                        .values()
                        .map(|child| positions(resources, base, child, refs))
                        .sum(),
                    (SchemaSelector::Array, Some(Value::Array(items))) => items
                        .iter()
                        .map(|child| positions(resources, base, child, refs))
                        .sum(),
                    _ => 0,
                };
            }
            count
        }
        let root = "https://openbindings.invalid/internal-meta/schema";
        assert_eq!(
            positions(&resources, root, &resources[root], &mut Vec::new()),
            129
        );
    }

    #[test]
    fn schemas_of_any_admitted_size_get_a_meta_schema_verdict() {
        // A long near miss of a meta pattern and a schema with many members
        // both decide; no work budget stops either.
        let n = 1_000_000;
        let long = "a".repeat(n);
        let members =
            |entry: &dyn Fn(usize) -> String| (0..200_000).map(entry).collect::<Vec<_>>().join(",");
        let decided = [
            (format!(r#"{{"$anchor":"{long}!"}}"#), false),
            (format!(r#"{{"$dynamicAnchor":"{long}!"}}"#), false),
            (format!(r#"{{"$id":"https://e.example/{long}##"}}"#), false),
            (format!(r#"{{"$anchor":"{long}"}}"#), true),
            (format!(r#"{{"$id":"https://e.example/{long}#"}}"#), true),
            (
                format!(
                    r#"{{"$vocabulary":{{{}}}}}"#,
                    members(&|i| format!(r#""https://e.example/{i}":true"#))
                ),
                true,
            ),
            (
                format!(r#"{{"required":[{}]}}"#, members(&|i| format!(r#""p{i}""#))),
                true,
            ),
            (
                format!(
                    r#"{{"properties":{{{}}}}}"#,
                    members(&|i| format!(r#""p{i}":{{}}"#))
                ),
                true,
            ),
        ];
        for (text, valid) in decided {
            let value = JsonValue::parse(&text).unwrap();
            assert_eq!(validity(&value, true), Ok(valid), "{}", &text[..24]);
        }
    }

    #[test]
    fn long_near_misses_of_every_fixed_pattern_fail() {
        // A near miss is where a backtracking matcher does the most work. Each
        // one still reaches its verdict: the patterns run on a finite automaton.
        let n = 100_000;
        let meta = [
            format!(r#"{{"$anchor":"{}!"}}"#, "a".repeat(n)),
            format!(r#"{{"$anchor":"{}!"}}"#, "a.-".repeat(n)),
            format!(r#"{{"$dynamicAnchor":"{}!"}}"#, "_9".repeat(n)),
            format!(r#"{{"$id":"{}##"}}"#, "a".repeat(n)),
            format!(r#"{{"$id":"{}#a"}}"#, "/".repeat(n)),
        ];
        for text in &meta {
            let value = JsonValue::parse(text).unwrap();
            assert_eq!(validity(&value, true), Ok(false), "{}", &text[..24]);
        }
        let version =
            |body: &str| format!(r#"{{"openbindings":"0.0.0{body}!","operations":{{}}}}"#);
        let mut documents: Vec<String> = ["1", "1a", "-", "00a", "0a-1.", "1.", "a."]
            .iter()
            .map(|unit| version(&format!("-{}", unit.repeat(n))))
            .collect();
        documents.push(version(&format!("+{}", "a.".repeat(n))));
        documents.push(format!(
            "{{\"openbindings\":\"{}!.0.0\",\"operations\":{{}}}}",
            "1".repeat(n)
        ));
        for name in [
            "a".repeat(n) + "!",
            "a.-".repeat(n) + "!",
            "!".to_owned() + &"a".repeat(n),
        ] {
            documents.push(format!(
                r#"{{"openbindings":"0.2.0","operations":{{"{name}":{{}}}}}}"#
            ));
            documents.push(format!(
                r#"{{"openbindings":"0.2.0","operations":{{"o":{{"aliases":["{name}"]}}}}}}"#
            ));
        }
        for text in &documents {
            let value = JsonValue::parse(text).unwrap();
            assert_eq!(validity(&value, false), Ok(false), "{}", &text[..48]);
        }
    }

    #[test]
    fn fixed_checks_run_their_patterns_on_the_default_engines_finite_automaton() {
        // The library's default pattern engine accepts lookaround, which a
        // finite-automaton engine alone refuses...
        assert!(compile(&serde_json::json!({"pattern": "(?<=a)b"}), BTreeMap::new()).is_ok());
        // ...and hands a regular pattern whole to its finite automaton. This
        // valid version makes a backtracking matcher retreat over two million
        // digits before the letter; on the engine's backtracking path that
        // exceeds its default limit of a million steps, which this library
        // release reads as a non-match.
        let version = JsonValue::parse(format!(
            r#"{{"openbindings":"0.0.0-{}a","operations":{{}}}}"#,
            "1".repeat(2_000_000)
        ))
        .unwrap();
        assert_eq!(validity(&version, false), Ok(true));
    }

    #[test]
    fn fixed_schemas_assert_only_exact_comparisons_of_small_integers() {
        // Only the four comparisons are registered as exact keywords; a fixed
        // `multipleOf` would fall to the library's binary64 arithmetic.
        use crate::schema_index::{SCHEMA_SELECTORS, SchemaSelector};
        fn walk(schema: &Value, operands: &mut Vec<String>) {
            let Some(object) = schema.as_object() else {
                return;
            };
            assert!(!object.contains_key("multipleOf"));
            for keyword in numeric::COMPARISONS {
                if let Some(operand) = object.get(keyword) {
                    operands.push(operand.to_string());
                }
            }
            for (keyword, selector) in SCHEMA_SELECTORS {
                match (selector, object.get(*keyword)) {
                    (SchemaSelector::Direct, Some(child)) => walk(child, operands),
                    (
                        SchemaSelector::Map | SchemaSelector::MapOrStringArray,
                        Some(Value::Object(map)),
                    ) => map.values().for_each(|child| walk(child, operands)),
                    (SchemaSelector::Array, Some(Value::Array(items))) => {
                        items.iter().for_each(|child| walk(child, operands))
                    }
                    _ => {}
                }
            }
        }
        let mut operands = Vec::new();
        let document = include_str!("../schemas/openbindings.schema.json");
        for text in [document].into_iter().chain(META_FILES) {
            walk(&serde_json::from_str(text).unwrap(), &mut operands);
        }
        assert!(!operands.is_empty());
        for operand in operands {
            let exact = operand
                .parse::<i64>()
                .is_ok_and(|n| n.unsigned_abs() < 1 << 53);
            assert!(exact, "{operand}");
        }
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
        let already = check_with_retained(value.view(), true, 0, |at, message| {
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

    #[test]
    fn meta_failures_are_located_at_each_failing_keyword() {
        let located = |text: &str| {
            let value = JsonValue::parse(text).unwrap();
            let problems = check(&value, true, 100).unwrap();
            assert!(problems.violated, "{text}");
            problems
                .entries
                .iter()
                .map(|p| {
                    let pointer = p.at.location().pointer.unwrap();
                    (pointer.as_str().unwrap().to_owned(), p.message.to_string())
                })
                .collect::<Vec<_>>()
        };
        let keyword = |name: &str| {
            format!(
                "the value of the {name} keyword does not satisfy the JSON Schema 2020-12 meta-schema"
            )
        };
        assert_eq!(
            located(r#"{"items":5,"title":"ok","type":"bogus","x-extra":0}"#),
            [
                ("/items".to_owned(), keyword("items")),
                ("/type".to_owned(), keyword("type")),
            ]
        );
        assert_eq!(
            located(r##"{"$id":"#x"}"##),
            [("/$id".to_owned(), keyword("$id"))]
        );
        for text in ["null", "7", r#""schema""#, "[]"] {
            assert_eq!(
                located(text),
                [(String::new(), SCHEMA_TYPE_MESSAGE.to_owned())]
            );
        }
        // The guidance shares the findings allowance and its truncation.
        let value = JsonValue::parse(r#"{"items":5,"type":"bogus"}"#).unwrap();
        let bounded = check(&value, true, 1).unwrap();
        assert!(bounded.violated && bounded.truncated);
        assert_eq!(bounded.entries.len(), 1);
    }

    #[test]
    fn each_keyword_check_decides_as_the_meta_schemas_do() {
        // A one-keyword schema conforms exactly when its value passes that
        // keyword's own check, so locating at the keywords never invents or
        // misses a failure.
        let meta = meta_keywords().as_ref().unwrap();
        let validator = super::meta().as_ref().unwrap();
        assert_eq!(meta.keywords.len(), 61);
        let values = [
            "null",
            "true",
            "0",
            "-1",
            "1.5",
            r#""x""#,
            r##""#x""##,
            r#""https://e.example/s""#,
            "[]",
            "[1]",
            r#"["a"]"#,
            r#"["a","a"]"#,
            "[{}]",
            "{}",
            r#"{"a":5}"#,
            r#"{"a":true}"#,
            r#"{"a":["b"]}"#,
            r#"{"https://e.example/v":true}"#,
        ];
        for (keyword, (key, _)) in &meta.keywords {
            let own = meta.validators.get(key).unwrap();
            for text in values {
                let raw = format!("{{{}:{text}}}", serde_json::to_string(keyword).unwrap());
                let schema = JsonValue::parse(&raw).unwrap();
                let member = JsonValue::parse(text).unwrap();
                assert_eq!(
                    backend::fixed_strings(|| valid(own, member.view())),
                    backend::fixed_strings(|| valid(validator, schema.view())),
                    "{raw}"
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/support/meta_equivalence.rs"]
mod equivalence;

#[cfg(test)]
#[path = "../tests/support/ecma_patterns.rs"]
mod ecma_patterns;
