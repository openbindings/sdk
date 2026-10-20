// The fixed checks' patterns match exactly as ECMAScript's `u`-mode RegExp
// does, through every keyword that can carry them and under the fixed-string
// presentation of unpaired surrogates. `fixed-pattern-ecma.json` records V8's
// results over a corpus that `fixed-pattern-ecma.mjs` generates from a seed;
// this module generates the same corpus, checks its digest, and compares the
// shipped engine with every recorded result. The TypeScript suite regenerates
// the recorded results from V8 on every run.
use super::*;
use crate::schema_index::{SCHEMA_SELECTORS, SchemaSelector};
use serde_json::json;
use std::collections::{BTreeSet, HashSet};
use std::fmt::Write;

const FIXTURE: &str = include_str!("../fixtures/fixed-pattern-ecma.json");

struct Spec {
    seed: u32,
    size: usize,
    patterns: Vec<String>,
    pieces: Vec<Vec<u16>>,
    versions: Vec<Vec<u16>>,
    bases: Vec<Vec<u16>>,
    decorations: Vec<Vec<u16>>,
    prerelease: Vec<Vec<u16>>,
    build: Vec<Vec<u16>>,
    digest: String,
    matches: Vec<Vec<u8>>,
}
fn spec(fixture: &JsonValue) -> Spec {
    let list = |name: &str| -> Vec<Vec<u16>> {
        fixture
            .get(name)
            .and_then(|v| v.elements())
            .expect(name)
            .map(|v| v.utf16_units().expect("string"))
            .collect()
    };
    let text = |name: &str| fixture.get(name).and_then(|v| v.as_str()).expect(name);
    let number = |name: &str| {
        fixture
            .get(name)
            .and_then(|v| v.number_text())
            .and_then(|v| v.parse::<u64>().ok())
            .expect(name)
    };
    let strings = |name: &str| -> Vec<String> {
        fixture
            .get(name)
            .and_then(|v| v.elements())
            .expect(name)
            .map(|v| v.as_str().expect("scalar string").to_owned())
            .collect()
    };
    Spec {
        seed: number("seed") as u32,
        size: number("size") as usize,
        patterns: strings("patterns"),
        pieces: list("pieces"),
        versions: list("versions"),
        bases: list("bases"),
        decorations: list("decorations"),
        prerelease: list("prerelease"),
        build: list("build"),
        digest: text("digest").to_owned(),
        matches: strings("matches")
            .iter()
            .map(|hex| {
                (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect()
            })
            .collect(),
    }
}

// mulberry32, as the generator script computes it.
struct Generator(u32);
impl Generator {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = (self.0 ^ (self.0 >> 15)).wrapping_mul(1 | self.0);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) % n as u32) as usize
    }
    fn pick<'a>(&mut self, list: &'a [Vec<u16>]) -> &'a [u16] {
        &list[self.below(list.len())]
    }
    fn edit(&mut self, text: &[u16], pieces: &[Vec<u16>]) -> Vec<u16> {
        let mut units = text.to_vec();
        for _ in 0..1 + self.below(3) {
            let operation = self.below(3);
            let at = self.below(units.len() + 1);
            let piece = self.pick(pieces).to_vec();
            let last = at.min(units.len().saturating_sub(1));
            if operation == 0 {
                units.splice(at..at, piece);
            } else if !units.is_empty() && operation == 1 {
                units.remove(last);
            } else if !units.is_empty() {
                units.splice(last..=last, piece);
            }
        }
        units
    }
    fn number(&mut self) -> usize {
        let any = self.below(1000);
        [0, 1, 9, 10, any][self.below(5)]
    }
    fn join(&mut self, list: &[Vec<u16>], count: usize) -> Vec<u16> {
        let mut text = Vec::new();
        for i in 0..count {
            if i > 0 {
                text.push(u16::from(b'.'));
            }
            text.extend_from_slice(self.pick(list));
        }
        text
    }
}

#[derive(Default)]
struct Corpus {
    strings: Vec<Vec<u16>>,
    seen: HashSet<Vec<u16>>,
}
impl Corpus {
    fn add(&mut self, text: Vec<u16>) {
        if self.seen.insert(text.clone()) {
            self.strings.push(text);
        }
    }
}

fn corpus(spec: &Spec) -> Vec<Vec<u16>> {
    let mut generator = Generator(spec.seed);
    let mut corpus = Corpus::default();
    for text in spec
        .versions
        .iter()
        .chain(&spec.pieces)
        .chain([&Vec::new()])
    {
        corpus.add(text.clone());
        for decoration in &spec.decorations {
            corpus.add([text.as_slice(), decoration].concat());
            corpus.add([decoration.as_slice(), text].concat());
        }
    }
    while corpus.strings.len() < spec.size {
        match generator.below(4) {
            0 => {
                let mut text = Vec::new();
                for _ in 0..generator.below(13) {
                    text.extend_from_slice(generator.pick(&spec.pieces));
                }
                corpus.add(text);
            }
            1 => {
                let version = generator.pick(&spec.versions);
                corpus.add(generator.edit(version, &spec.pieces));
            }
            2 => {
                let base = generator.pick(&spec.bases);
                if generator.below(10) < 8 {
                    corpus.add(generator.edit(base, &spec.pieces));
                } else {
                    corpus.add(base.to_vec());
                }
            }
            _ => {
                let core = [generator.number(), generator.number(), generator.number()];
                let core = format!("{}.{}.{}", core[0], core[1], core[2]);
                let count = generator.below(5);
                let pre = generator.join(&spec.prerelease, count);
                let count = generator.below(4);
                let build = generator.join(&spec.build, count);
                let mut text: Vec<u16> = core.encode_utf16().collect();
                if !pre.is_empty() || generator.below(5) == 0 {
                    text.push(u16::from(b'-'));
                    text.extend(pre);
                }
                if !build.is_empty() || generator.below(5) == 0 {
                    text.push(u16::from(b'+'));
                    text.extend(build);
                }
                if generator.below(10) < 7 {
                    corpus.add(text);
                } else {
                    corpus.add(generator.edit(&text, &spec.pieces));
                }
            }
        }
    }
    corpus.strings
}

// FNV-1a over each string's length (four bytes) and units (two bytes each).
fn digest(strings: &[Vec<u16>]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut byte = |value: u8| hash = (hash ^ u64::from(value)).wrapping_mul(0x0100_0000_01b3);
    for text in strings {
        for value in (text.len() as u32).to_le_bytes() {
            byte(value);
        }
        for unit in text {
            for value in unit.to_le_bytes() {
                byte(value);
            }
        }
    }
    format!("{hash:016x}")
}

fn json_string(units: &[u16]) -> String {
    let mut out = String::from("\"");
    for &unit in units {
        match u8::try_from(unit) {
            Ok(byte @ 0x20..=0x7e) if byte != b'"' && byte != b'\\' => out.push(char::from(byte)),
            _ => write!(out, "\\u{unit:04x}").unwrap(),
        }
    }
    out.push('"');
    out
}

// Every `pattern` and `patternProperties` name at a schema position of the
// pinned document schema and meta-schemas.
fn pinned_patterns() -> BTreeSet<String> {
    fn walk(schema: &Value, out: &mut BTreeSet<String>) {
        let Some(object) = schema.as_object() else {
            return;
        };
        if let Some(pattern) = object.get("pattern").and_then(Value::as_str) {
            out.insert(pattern.to_owned());
        }
        if let Some(map) = object.get("patternProperties").and_then(Value::as_object) {
            out.extend(map.keys().cloned());
        }
        for (keyword, selector) in SCHEMA_SELECTORS {
            match (selector, object.get(*keyword)) {
                (SchemaSelector::Direct, Some(child)) => walk(child, out),
                (
                    SchemaSelector::Map | SchemaSelector::MapOrStringArray,
                    Some(Value::Object(map)),
                ) => map.values().for_each(|child| walk(child, out)),
                (SchemaSelector::Array, Some(Value::Array(items))) => {
                    items.iter().for_each(|child| walk(child, out))
                }
                _ => {}
            }
        }
    }
    let mut out = BTreeSet::new();
    let document = include_str!("../../schemas/openbindings.schema.json");
    for text in [document].into_iter().chain(META_FILES) {
        walk(&serde_json::from_str(text).unwrap(), &mut out);
    }
    out
}

#[test]
fn fixed_patterns_match_as_ecmascript_does_through_every_keyword() {
    let fixture = JsonValue::parse(FIXTURE).unwrap();
    let spec = spec(&fixture);
    assert_eq!(
        spec.patterns.iter().cloned().collect::<BTreeSet<_>>(),
        pinned_patterns(),
        "the fixture covers exactly the pinned schemas' patterns"
    );
    let strings = corpus(&spec);
    assert_eq!(strings.len(), spec.size);
    assert_eq!(
        digest(&strings),
        spec.digest,
        "the generator script's corpus"
    );
    let unpaired = strings
        .iter()
        .filter(|s| char::decode_utf16(s.iter().copied()).any(|c| c.is_err()))
        .count();
    let marker = strings.iter().filter(|s| s.contains(&0xe000)).count();
    assert!(unpaired > 5_000 && marker > 2_000, "{unpaired} {marker}");

    let build = |schema: Value| compile(&schema, BTreeMap::new()).unwrap();
    let mut mismatches = Vec::new();
    for (index, pattern) in spec.patterns.iter().enumerate() {
        // Each keyword path the library gives a pattern: a string value, a
        // member name, and a member name selecting a subschema with and without
        // a fused additionalProperties.
        let paths = [
            build(json!({"pattern": pattern})),
            build(json!({"propertyNames": {"pattern": pattern}})),
            build(json!({"patternProperties": {pattern.as_str(): false}})),
            build(
                json!({"patternProperties": {pattern.as_str(): true}, "additionalProperties": false}),
            ),
        ];
        for (i, text) in strings.iter().enumerate() {
            let expected = spec.matches[index][i / 8] >> (i % 8) & 1 == 1;
            let quoted = json_string(text);
            let value = JsonValue::parse(&quoted).unwrap();
            let member = JsonValue::parse(format!("{{{quoted}:0}}")).unwrap();
            let actual = backend::fixed_strings(|| {
                [
                    paths[0].is_valid(view(&value)),
                    paths[1].is_valid(view(&member)),
                    !paths[2].is_valid(view(&member)),
                    paths[3].is_valid(view(&member)),
                ]
            });
            for (path, matched) in actual.into_iter().enumerate() {
                if matched != expected && mismatches.len() < 20 {
                    mismatches.push((pattern.clone(), path, quoted.clone(), expected));
                }
            }
        }
    }
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}
