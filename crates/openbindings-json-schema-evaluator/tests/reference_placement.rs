//! Seeded random contracts whose schemas live in the document, in contained `$id` resources and
//! in supplied resources, against the same schemas as definitions of one supplied resource.
//! References cross between them through `properties`, applicators, `unevaluatedProperties`,
//! `unevaluatedItems` and `patternProperties`, often in long chains, so the library compiles
//! them over several rounds. Every value must evaluate in both placements, and wherever both
//! reach a verdict, the verdicts must agree.
use openbindings::*;
use openbindings_json_schema_evaluator::{DefaultEvaluator, Limits};

const DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";
const KEYS: [&str; 4] = ["a", "b", "c", "k"];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn range(&mut self, low: usize, high: usize) -> usize {
        low + self.below(high - low + 1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }
    fn sample<T: Copy>(&mut self, items: &[T], count: usize) -> Vec<T> {
        let mut pool = items.to_vec();
        (0..count)
            .map(|_| pool.remove(self.below(pool.len())))
            .collect()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Place {
    Document,
    Contained,
    Supplied,
}

#[derive(Clone, Copy)]
enum Fragment {
    Root,
    Definition,
    PropertyA,
    Anchor,
}

/// JSON in generation order; a reference is spelled by the placement it is rendered for.
enum Json {
    Text(String),
    Reference(usize, Fragment),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

fn text(json: &str) -> Json {
    Json::Text(json.to_owned())
}

fn object(members: Vec<(&str, Json)>) -> Json {
    Json::Object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn render(json: &Json, spell: &dyn Fn(usize, Fragment) -> String, out: &mut String) {
    match json {
        Json::Text(json) => out.push_str(json),
        Json::Reference(to, fragment) => out.push_str(&format!("\"{}\"", spell(*to, *fragment))),
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                render(item, spell, out);
            }
            out.push(']');
        }
        Json::Object(members) => {
            out.push('{');
            for (index, (key, value)) in members.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&format!("\"{key}\":"));
                render(value, spell, out);
            }
            out.push('}');
        }
    }
}

struct Contract {
    place: Vec<Place>,
    schemas: Vec<Json>,
}

struct Graph<'r> {
    rng: &'r mut Rng,
    place: Vec<Place>,
    anchors: Vec<bool>,
}

impl Graph<'_> {
    /// A reference from schema `from`; one applied in place only reaches later schemas, so no
    /// in-place cycle forms, while one through `properties` or `items` reaches any schema.
    fn reference(&mut self, from: usize, in_place: bool) -> Option<Json> {
        let low = if in_place { from + 1 } else { 0 };
        let targets: Vec<usize> = (low..self.place.len())
            // A resource cannot name a schema of the document by pointer.
            .filter(|&to| self.place[to] != Place::Document || self.place[from] == Place::Document)
            .collect();
        if targets.is_empty() {
            return None;
        }
        let to = self.rng.pick(&targets);
        let named = self.place[to] != Place::Document;
        let fragment = match self.rng.below(100) {
            0..15 if named => Fragment::Definition,
            15..25 if named => {
                self.anchors[to] = true;
                Fragment::Anchor
            }
            25..32 => Fragment::PropertyA,
            _ => Fragment::Root,
        };
        Some(Json::Reference(to, fragment))
    }

    fn leaf(&mut self) -> Json {
        text(self.rng.pick(&[
            r#"{"type":"string"}"#,
            r#"{"type":"integer"}"#,
            r#"{"type":"object"}"#,
            r#"{"type":"array"}"#,
            r#"{"minimum":1}"#,
            r#"{"const":1}"#,
            r#"{"const":"x"}"#,
            r#"{"enum":[1,2,"x"]}"#,
            r#"{"maxLength":2}"#,
            "true",
            "false",
            "{}",
        ]))
    }

    fn body(&mut self, from: usize, depth: usize, in_place_ok: bool) -> Json {
        if depth > 2 {
            return self.leaf();
        }
        let mut members: Vec<(&str, Json)> = Vec::new();
        for _ in 0..self.rng.range(1, 4) {
            let keyword = self.rng.pick(&[
                "type",
                "properties",
                "properties",
                "required",
                "additionalProperties",
                "patternProperties",
                "items",
                "prefixItems",
                "contains",
                "unevaluatedProperties",
                "unevaluatedProperties",
                "unevaluatedItems",
                "allOf",
                "anyOf",
                "oneOf",
                "not",
                "if",
                "dependentSchemas",
                "$ref",
                "$ref",
                "$dynamicRef",
            ]);
            if members.iter().any(|(present, _)| *present == keyword) {
                continue;
            }
            let child = depth + 1;
            let member = match keyword {
                "type" => text(self.rng.pick(&[
                    r#""object""#,
                    r#""array""#,
                    r#"["object","array"]"#,
                    r#""integer""#,
                ])),
                "properties" => {
                    let count = self.rng.range(1, 3);
                    let keys = self.rng.sample(&KEYS, count);
                    let properties = keys
                        .into_iter()
                        .map(|key| (key, self.sub(from, child, false)))
                        .collect();
                    object(properties)
                }
                "required" => {
                    let count = self.rng.range(1, 2);
                    let keys = self.rng.sample(&KEYS, count);
                    Json::Array(keys.into_iter().map(quoted).collect())
                }
                "additionalProperties" if self.rng.chance(50) => text("false"),
                "additionalProperties" => self.sub(from, child, false),
                "patternProperties" => object(vec![("^k", self.sub(from, child, false))]),
                "items" | "contains" => self.sub(from, child, false),
                "prefixItems" => Json::Array(
                    (0..self.rng.range(1, 2))
                        .map(|_| self.sub(from, child, false))
                        .collect(),
                ),
                "unevaluatedProperties" | "unevaluatedItems" if self.rng.chance(60) => {
                    text("false")
                }
                "unevaluatedProperties" | "unevaluatedItems" => self.sub(from, child, false),
                "allOf" | "anyOf" | "oneOf" if in_place_ok => Json::Array(
                    (0..self.rng.range(1, 2))
                        .map(|_| self.sub(from, child, true))
                        .collect(),
                ),
                "not" if in_place_ok => self.sub(from, child, true),
                "if" if in_place_ok => {
                    members.push(("if", self.sub(from, child, true)));
                    if self.rng.chance(80) {
                        members.push(("then", self.sub(from, child, true)));
                    }
                    if self.rng.chance(60) {
                        members.push(("else", self.sub(from, child, true)));
                    }
                    continue;
                }
                "dependentSchemas" if in_place_ok => {
                    let key = self.rng.pick(&KEYS);
                    object(vec![(key, self.sub(from, child, true))])
                }
                "$ref" | "$dynamicRef" if in_place_ok => match self.reference(from, true) {
                    Some(reference) => reference,
                    None => continue,
                },
                _ => continue,
            };
            members.push((keyword, member));
        }
        object(members)
    }

    /// A child: a reference to another schema, sometimes beside `unevaluatedProperties`, or a body.
    fn sub(&mut self, from: usize, depth: usize, in_place: bool) -> Json {
        if self.rng.chance(45)
            && let Some(reference) = self.reference(from, in_place)
        {
            let keyword = if self.rng.chance(15) {
                "$dynamicRef"
            } else {
                "$ref"
            };
            let mut members = vec![(keyword, reference)];
            if self.rng.chance(30) {
                members.push(("unevaluatedProperties", text("false")));
            }
            return object(members);
        }
        self.body(from, depth, in_place)
    }
}

fn quoted(key: &str) -> Json {
    Json::Text(format!("\"{key}\""))
}

/// Schemas referencing each other in place forward only, through `properties` anywhere.
fn graph(rng: &mut Rng) -> Contract {
    let count = rng.range(3, 7);
    let place = (0..count)
        .map(|_| rng.pick(&[Place::Document, Place::Contained, Place::Supplied]))
        .collect();
    let mut graph = Graph {
        rng,
        place,
        anchors: vec![false; count],
    };
    let mut schemas: Vec<Vec<(String, Json)>> = (0..count)
        .map(|index| match graph.body(index, 0, true) {
            Json::Object(members) => members,
            other => vec![("allOf".to_owned(), Json::Array(vec![other]))],
        })
        .collect();
    for (index, members) in schemas.iter_mut().enumerate() {
        if !members.iter().any(|(key, _)| key == "properties") {
            let a = graph.sub(index, 2, false);
            members.push(("properties".to_owned(), object(vec![("a", a)])));
        } else if let Some((_, Json::Object(properties))) =
            members.iter_mut().find(|(key, _)| key == "properties")
            && !properties.iter().any(|(key, _)| key == "a")
        {
            properties.push(("a".to_owned(), graph.sub(index, 2, false)));
        }
        if graph.place[index] != Place::Document {
            let mut definitions = vec![("d0".to_owned(), graph.sub(index, 2, false))];
            if graph.anchors[index] {
                definitions.push((
                    format!("an{index}"),
                    object(vec![
                        ("type", text(r#""object""#)),
                        ("$anchor", quoted(&format!("an{index}"))),
                    ]),
                ));
            }
            members.push(("$defs".to_owned(), Json::Object(definitions)));
        }
    }
    Contract {
        place: graph.place,
        schemas: schemas.into_iter().map(Json::Object).collect(),
    }
}

/// Chains and diamonds of resources longer than the library compiles on its stack, with back
/// edges, `patternProperties` to any link and unevaluated keywords at random links.
fn chain(rng: &mut Rng) -> Contract {
    let count = rng.range(8, 16);
    let width = rng.pick(&[1, 1, 2]);
    // All supplied, all contained, or each link either.
    let mode = rng.below(3);
    let place = (0..count)
        .map(|_| match mode {
            0 => Place::Supplied,
            1 => Place::Contained,
            _ => rng.pick(&[Place::Supplied, Place::Contained]),
        })
        .collect();
    let mut schemas = Vec::with_capacity(count);
    for index in 0..count {
        let next: Vec<usize> = (index + 1..count.min(index + 1 + width)).collect();
        let reference = |to: usize| object(vec![("$ref", Json::Reference(to, Fragment::Root))]);
        let mut members: Vec<(&str, Json)> = Vec::new();
        let mut properties: Vec<(&str, Json)> = Vec::new();
        if !next.is_empty() {
            match rng.below(5) {
                0 => {
                    for &to in &next {
                        let key = rng.pick(&["a", "b", "c"]);
                        properties.retain(|(k, _)| *k != key);
                        properties.push((key, reference(to)));
                    }
                }
                1 => members.push((
                    "allOf",
                    Json::Array(next.iter().map(|&to| reference(to)).collect()),
                )),
                2 => {
                    let mut branches: Vec<Json> = next.iter().map(|&to| reference(to)).collect();
                    branches.push(text(r#"{"type":"string"}"#));
                    members.push(("anyOf", Json::Array(branches)));
                }
                3 => members.push(("items", reference(next[0]))),
                _ => members.push(("$ref", Json::Reference(next[0], Fragment::Root))),
            }
        }
        if rng.chance(35) {
            let back = rng.below(index + 1);
            properties.retain(|(k, _)| *k != "z");
            properties.push(("z", reference(back)));
        }
        if rng.chance(30) {
            let to = rng.below(count);
            members.push(("patternProperties", object(vec![("^k", reference(to))])));
        }
        if rng.chance(30) {
            members.push(("unevaluatedProperties", text("false")));
        }
        if rng.chance(15) {
            members.push(("unevaluatedItems", text("false")));
        }
        if rng.chance(30) {
            let key = rng.pick(&["a", "b", "c", "k"]);
            let leaf = rng.pick(&[r#"{"type":"integer"}"#, r#"{"const":1}"#, "true", "false"]);
            properties.retain(|(k, _)| *k != key);
            properties.push((key, text(leaf)));
        }
        if rng.chance(20) {
            let key = rng.pick(&["a", "b", "c"]);
            members.push(("required", Json::Array(vec![quoted(key)])));
        }
        if !properties.is_empty() {
            members.push(("properties", object(properties)));
        }
        schemas.push(object(members));
    }
    Contract { place, schemas }
}

fn uri(index: usize) -> String {
    format!("https://placement.test/S{index}")
}

fn with_first(
    schema: &Json,
    first: (&str, String),
    spell: &dyn Fn(usize, Fragment) -> String,
) -> String {
    let mut text = format!("{{\"{}\":\"{}\"", first.0, first.1);
    let mut rest = String::new();
    render(schema, spell, &mut rest);
    if rest != "{}" {
        text.push(',');
        text.push_str(&rest[1..rest.len() - 1]);
    }
    text.push('}');
    text
}

/// The contract with every schema where its placement puts it.
fn split(contract: &Contract) -> (String, Vec<(String, String)>) {
    let place = &contract.place;
    let spell = |to: usize, fragment: Fragment| match (place[to], fragment) {
        (Place::Document, Fragment::PropertyA) => format!("#/schemas/S{to}/properties/a"),
        (Place::Document, _) => format!("#/schemas/S{to}"),
        (_, Fragment::Root) => uri(to),
        (_, Fragment::Definition) => format!("{}#/$defs/d0", uri(to)),
        (_, Fragment::PropertyA) => format!("{}#/properties/a", uri(to)),
        (_, Fragment::Anchor) => format!("{}#an{to}", uri(to)),
    };
    let mut document = Vec::new();
    let mut supplied = Vec::new();
    for (index, schema) in contract.schemas.iter().enumerate() {
        match place[index] {
            Place::Document => {
                let mut text = String::new();
                render(schema, &spell, &mut text);
                document.push(format!("\"S{index}\":{text}"));
            }
            Place::Contained => {
                document.push(format!(
                    "\"S{index}\":{}",
                    with_first(schema, ("$id", uri(index)), &spell)
                ));
            }
            Place::Supplied => supplied.push((
                uri(index),
                with_first(schema, ("$schema", DRAFT.to_owned()), &spell),
            )),
        }
    }
    let entry = spell(0, Fragment::Root);
    let schemas = if document.is_empty() {
        String::new()
    } else {
        format!(",\"schemas\":{{{}}}", document.join(","))
    };
    (
        format!(
            r#"{{"openbindings":"0.2.0","operations":{{"op":{{"input":{{"$ref":"{entry}"}}}}}}{schemas}}}"#
        ),
        supplied,
    )
}

/// The same schemas as definitions of one supplied resource.
fn single(contract: &Contract) -> (String, Vec<(String, String)>) {
    let spell = |to: usize, fragment: Fragment| match fragment {
        Fragment::Root => format!("#/$defs/S{to}"),
        Fragment::Definition => format!("#/$defs/S{to}/$defs/d0"),
        Fragment::PropertyA => format!("#/$defs/S{to}/properties/a"),
        Fragment::Anchor => format!("#an{to}"),
    };
    let mut definitions = Vec::new();
    for (index, schema) in contract.schemas.iter().enumerate() {
        let mut text = String::new();
        render(schema, &spell, &mut text);
        definitions.push(format!("\"S{index}\":{text}"));
    }
    let resource = format!(
        r#"{{"$schema":"{DRAFT}","$defs":{{{}}}}}"#,
        definitions.join(",")
    );
    (
        r#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://placement.test/one#/$defs/S0"}}}}"#.to_owned(),
        vec![("https://placement.test/one".to_owned(), resource)],
    )
}

fn value(rng: &mut Rng, depth: usize, out: &mut String) {
    let roll = rng.below(100);
    if depth > 3 || roll < 25 {
        out.push_str(rng.pick(&[
            "1", "2", "0", "-1", "2.5", "\"x\"", "\"yy\"", "\"\"", "true", "false", "null",
        ]));
    } else if roll < 65 {
        let count = rng.range(0, 3);
        let keys = rng.sample(&["a", "b", "c", "k", "z"], count);
        out.push('{');
        for (index, key) in keys.into_iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&format!("\"{key}\":"));
            value(rng, depth + 1, out);
        }
        out.push('}');
    } else {
        out.push('[');
        for index in 0..rng.range(0, 3) {
            if index > 0 {
                out.push(',');
            }
            value(rng, depth + 1, out);
        }
        out.push(']');
    }
}

fn prepare(
    evaluator: &DefaultEvaluator,
    (document, supplied): &(String, Vec<(String, String)>),
) -> Result<PreparedValueContract, NoVerdict> {
    let parsed = Document::parse(document).unwrap();
    let validation = parsed.validate().unwrap();
    let proof = validation
        .conformant()
        .unwrap_or_else(|| panic!("conformant document {document}"));
    let resources = ResourceSet::new(supplied.iter().map(|(uri, text)| SchemaResource {
        uri: uri.clone(),
        document: JsonValue::parse(text).unwrap(),
    }))
    .unwrap();
    proof
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare_with(evaluator, &resources)
}

/// `Some(true)` satisfies, `Some(false)` fails, `None` reaches no verdict.
fn verdict(outcome: &ValueOutcome) -> Option<bool> {
    match outcome {
        ValueOutcome::Satisfies => Some(true),
        ValueOutcome::Fails { .. } => Some(false),
        ValueOutcome::Undefined { .. } | ValueOutcome::NoVerdict { .. } => None,
    }
}

const CASES: usize = 500;

#[test]
fn verdicts_do_not_depend_on_where_referenced_schemas_live() {
    let mut rng = Rng(0x2f6b_95d1_c0a4_7e33);
    // Diamonds can take exponentially many evaluation paths; a value that needs more work than
    // this reaches no verdict in either placement and is not compared.
    let evaluator = DefaultEvaluator::with_limits(Limits {
        evaluation_steps: Some(20_000),
        ..Limits::default()
    });
    let mut compared = 0;
    for case in 0..CASES {
        let contract = if case % 3 == 2 {
            chain(&mut rng)
        } else {
            graph(&mut rng)
        };
        let placed = split(&contract);
        let control = single(&contract);
        let (placed_contract, control_contract) =
            (prepare(&evaluator, &placed), prepare(&evaluator, &control));
        for _ in 0..12 {
            let mut text = String::new();
            value(&mut rng, 0, &mut text);
            let value = JsonValue::parse(&text).unwrap();
            let mut verdicts = Vec::new();
            for contract in [&placed_contract, &control_contract].into_iter().flatten() {
                let checked = verdict(&contract.validate(&value));
                assert_eq!(
                    checked,
                    verdict(&contract.explain(&value)),
                    "{}\n{text}",
                    placed.0
                );
                verdicts.push(checked);
            }
            if let [Some(placed_verdict), Some(control_verdict)] = verdicts[..] {
                assert_eq!(
                    placed_verdict, control_verdict,
                    "{}\n{:?}\n{text}",
                    placed.0, placed.1
                );
                compared += 1;
            }
        }
    }
    assert!(compared > CASES * 6, "{compared} verdicts compared");
}
