//! Default-limit, boundary-biased lane differential. Keep raw numeric tokens exact.
use super::*;

// Reuse reference_placement.rs's xorshift sequence without changing its capped test.
struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
    fn pick<'a>(&mut self, values: &'a [&str]) -> &'a str {
        values[self.below(values.len())]
    }
}

struct Case {
    schema: String,
    values: Vec<String>,
}
impl Case {
    fn new(schema: String, values: &[&str]) -> Self {
        Self {
            schema,
            values: values.iter().map(|s| (*s).into()).collect(),
        }
    }
}

const NUMBERS: &[&[&str]] = &[
    &[
        "-9223372036854775809",
        "-9223372036854775808",
        "-9223372036854775807",
    ],
    &[
        "9223372036854775806",
        "9223372036854775807",
        "9223372036854775808",
    ],
    &["9007199254740991", "9007199254740992", "9007199254740993"],
    &["-0.1", "-0", "0.1"],
    &["0.29999999999999999", "0.3", "0.30000000000000001"],
    &["9.99e-101", "1e-100", "1.001e-100"],
    &["9.99e100", "1e101", "1.001e101"],
];
// Each row is an equal pair and a distinct neighbor, including every JSON type.
const LITERALS: &[&[&str]] = &[
    &["null", "null", "false"],
    &["true", "true", "false"],
    &["-0", "0.0e9", "1e-9"],
    &[
        "9223372036854775808",
        "9223372036854775808.0",
        "9223372036854775809",
    ],
    &[
        r#""a\n\"😀""#,
        r#""\u0061\n\"\ud83d\ude00""#,
        r#""a\n\"😁""#,
    ],
    &[
        r#"[1,{"a":-0}]"#,
        r#"[1.0,{"a":0e2}]"#,
        r#"[1,{"a":0},null]"#,
    ],
    &[
        r#"{"a":1,"b":[null,true]}"#,
        r#"{"b":[null,true],"a":1e0}"#,
        r#"{"a":1,"b":[null,false]}"#,
    ],
];
const PATTERNS: &[&str] = &[
    r"^a+$",
    r"^[a-z]{1,3}$",
    r"^\d{2}$",
    r"^.$",
    r"^😀$",
    r"^a\n$",
    r"^(ab|😀)$",
];

fn generate(rng: &mut Rng, index: usize) -> Case {
    let family = index % 12;
    let center = rng.below(4001) as i64 - 2000;
    let exponent = rng.pick(&["-18", "-6", "-1", "0", "1", "18"]);
    let generated = [center - 1, center, center + 1].map(|n| format!("{n}e{exponent}"));
    let generated_refs = generated.each_ref().map(String::as_str);
    let choice = rng.below(NUMBERS.len() + 1);
    let row = NUMBERS.get(choice).copied().unwrap_or(&generated_refs);
    let literal = LITERALS[(index / 12) % LITERALS.len()];
    let pattern = serde_json::to_string(rng.pick(PATTERNS)).unwrap();
    let mut case = match family {
        0 => {
            let low = rng.pick(&["minimum", "exclusiveMinimum"]);
            let high = rng.pick(&["maximum", "exclusiveMaximum"]);
            let kind = rng.pick(&["number", "integer"]);
            Case::new(format!(r#"{{"type":"{kind}","{low}":{},"{high}":{}}}"#, row[0], row[1]), row)
        }
        1 => {
            let (divisor, values): (&str, &[&str]) = match rng.below(4) {
                0 => ("0.1", &["-0", "0.2", "0.3", "0.30000000000000001", "-0.3"]),
                1 => ("3", &["9223372036854775806", "9223372036854775807", "9223372036854775808", "-9223372036854775809"]),
                2 => ("1e-100", &["-0", "3e-100", "3.01e-100", "3e-101"]),
                _ => ("2.5", &["-2.5", "0", "2.49", "2.5", "5.0e0"]),
            };
            Case::new(format!(r#"{{"multipleOf":{divisor}}}"#), values)
        }
        2 => Case::new(format!(r#"{{"const":{}}}"#, literal[0]), literal),
        3 => Case::new(format!(r#"{{"enum":[{},{}]}}"#, literal[0], literal[2]), literal),
        4 => {
            let length = rng.below(4);
            Case::new(format!(r#"{{"type":"string","minLength":{length},"maxLength":{},"pattern":{pattern}}}"#, length + 1),
                &[r#""""#, r#""a""#, r#""aa""#, r#""aaa""#, r#""aaaa""#, r#""12""#, r#""😀""#, r#""\ud83d\ude00""#, r#""a\n""#, r#""\"\\""#])
        }
        5 => Case::new(format!(r#"{{"type":"object","required":["a"],"properties":{{"a":{{"minimum":{}}}}},"patternProperties":{{{pattern}:{{"type":"integer"}}}},"additionalProperties":{},"propertyNames":{{"minLength":1,"maxLength":3}}}}"#, row[1], rng.below(2) == 0),
            &[r#"{}"#, r#"{"a":0}"#, r#"{"a":1,"b":2}"#, r#"{"a":1,"":0}"#, r#"{"a":1,"abcd":0}"#, r#"{"a":1,"😀":2}"#, r#"{"a":1,"12":2.5}"#, r#"{"a":1,"aa":"x"}"#]),
        6 => {
            let mut case = Case::new(format!(r#"{{"type":"array","uniqueItems":true,"minItems":{},"maxItems":3}}"#, rng.below(3)), &["[]"]);
            case.values.extend([
                format!("[{}]", literal[0]),
                format!("[{},{}]", literal[0], literal[1]),
                format!("[{},{}]", literal[0], literal[2]),
                format!("[{},null,false,true]", literal[0]),
            ]);
            case
        }
        7 => {
            let op = rng.pick(&["oneOf", "anyOf"]);
            Case::new(format!(r#"{{"{op}":[{{"maximum":{}}},{{"minimum":{}}}]}}"#, row[1], row[1]), row)
        }
        8 => Case::new(format!(r#"{{"not":{{"const":{}}}}}"#, literal[0]), literal),
        9 => Case::new(format!(r#"{{"if":{{"type":"number"}},"then":{{"exclusiveMinimum":{}}},"else":{{"const":{}}}}}"#, row[1], literal[0]), &[row[0], row[1], row[2], literal[0], literal[2]]),
        10 => Case::new(format!(r##"{{"$defs":{{"node":{{"anyOf":[{{"const":{}}},{{"type":"array","items":{{"$ref":"#/$defs/node"}},"maxItems":2}}]}}}},"$ref":"#/$defs/node"}}"##, literal[0]),
            &["null", "[]", "[[]]", "[[[]]]", "[null,null]", "[[],[],[]]", "[[1]]"]),
        _ => Case::new(r##"{"$defs":{"node":{"type":"object","properties":{"next":{"$ref":"#/$defs/node"},"n":{"type":"integer","minimum":0}},"required":["n"],"additionalProperties":false}},"$ref":"#/$defs/node"}"##.into(),
            &[r#"{"n":0}"#, r#"{"n":-0.0}"#, r#"{"n":-1}"#, r#"{"n":0,"next":{"n":1}}"#, r#"{"n":0,"next":{}}"#, r#"{"n":0,"extra":null}"#]),
    };
    if family == 5 {
        case.values
            .extend(row.iter().map(|v| format!(r#"{{"a":{v}}}"#)));
    }
    if family == 10 {
        case.values.extend([
            literal[0].to_owned(),
            format!("[{}]", literal[0]),
            format!("[[{}]]", literal[0]),
        ]);
    }
    // Add cross-type probes without replacing the boundary-biased majority.
    case.values
        .extend(["null".into(), "false".into(), "{}".into()]);
    if family < 10 {
        match rng.below(5) {
            0 => {}
            1 | 2 => {
                // Chains and shared diamonds. All refs are rooted in this resource.
                let depth = 1 + rng.below(5);
                let diamond = rng.below(2) == 0;
                let mut definitions = vec![format!(r#""d0":{}"#, case.schema)];
                for level in 1..=depth {
                    let reference = format!(r##"{{"$ref":"#/$defs/d{}"}}"##, level - 1);
                    let body = if diamond {
                        format!(r#"{{"allOf":[{reference},{reference}]}}"#)
                    } else {
                        reference
                    };
                    definitions.push(format!(r#""d{level}":{body}"#));
                }
                case.schema = format!(
                    r##"{{"$defs":{{{}}},"$ref":"#/$defs/d{depth}"}}"##,
                    definitions.join(",")
                );
            }
            3 => {
                case.schema = format!(
                    r#"{{"type":"array","prefixItems":[{}],"items":false}}"#,
                    case.schema
                );
                case.values = case.values.iter().map(|v| format!("[{v}]")).collect();
                case.values.extend(["[]".into(), "[null,null]".into()]);
            }
            _ => {
                case.schema = format!(
                    r#"{{"type":"object","properties":{{"a":{}}},"required":["a"],"additionalProperties":false}}"#,
                    case.schema
                );
                case.values = case
                    .values
                    .iter()
                    .map(|v| format!(r#"{{"a":{v}}}"#))
                    .collect();
                case.values
                    .extend(["{}".into(), r#"{"a":null,"b":0}"#.into()]);
            }
        }
    }
    case
}

fn verdict(outcome: &ValueOutcome) -> Option<bool> {
    match outcome {
        ValueOutcome::Satisfies => Some(true),
        ValueOutcome::Fails { .. } => Some(false),
        ValueOutcome::Undefined { .. } | ValueOutcome::NoVerdict { .. } => None,
    }
}

#[test]
fn default_limit_lane_differential() {
    let cases =
        std::env::var("OB_DIFFERENTIAL_CASES").map_or(1008, |s| s.parse::<usize>().unwrap());
    let seeds =
        std::env::var("OB_DIFFERENTIAL_SEEDS").unwrap_or_else(|_| "2f6b95d1c0a47e33".into());
    assert!(cases >= 84, "at least one cycle of all literal types");
    for seed in seeds.split(',') {
        let seed = u64::from_str_radix(seed.trim().trim_start_matches("0x"), 16).unwrap();
        assert_ne!(seed, 0, "xorshift needs a nonzero seed");
        let mut rng = Rng(seed);
        let (mut comparisons, mut compiled, mut no_verdict) = (0, 0, 0);
        for index in 0..cases {
            let case = generate(&mut rng, index);
            let resources = ExternalSchemas::new([openbindings::ExternalSchema {
                uri: ("https://generated.test/root").into(),
                document: JsonValue::parse(&case.schema).unwrap(),
            }])
            .unwrap();
            let validator = prepare_document(
                &document(r#"{"$ref":"https://generated.test/root"}"#),
                &resources,
                &DefaultEvaluator::with_limits(Limits::default()),
                &PreparationLimits::default(),
            )
            .unwrap_or_else(|_| {
                panic!("generated schema preparation failed at seed={seed:x} case={index}")
            });
            let original = prepare_document(
                &document(r#"{"$ref":"https://generated.test/root"}"#),
                &resources,
                &Original(Limits::default()),
                &PreparationLimits::default(),
            )
            .unwrap();
            for text in &case.values {
                let value = JsonValue::parse(text).unwrap();
                compiled::entries::take();
                let evaluated = validator.evaluate(&value, Detail::Outcome, &HEALTHY);
                let entries = compiled::entries::take();
                assert_eq!(
                    serialized(evaluated.clone()),
                    serialized(original.evaluate(&value, Detail::Outcome, &HEALTHY)),
                    "independent evaluation outcomes must agree"
                );
                let explained = validator.evaluate(&value, Detail::Explain, &HEALTHY);
                if let (Some(a), Some(b)) = (verdict(&evaluated), verdict(&explained)) {
                    assert_eq!(
                        a, b,
                        "evaluation details disagree at seed={seed:x} case={index}"
                    );
                    comparisons += 1;
                    compiled += entries[2];
                } else {
                    no_verdict += 1;
                }
            }
        }
        eprintln!(
            "lane differential seed={seed:x} schemas={cases} comparisons={comparisons} compiled={compiled} no_verdict={no_verdict}"
        );
        assert!(comparisons >= cases * 5, "{comparisons} comparisons");
        assert!(compiled > 0, "{compiled} compiled comparisons");
    }
}
