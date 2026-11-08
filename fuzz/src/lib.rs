//! Shared target bodies for libFuzzer and the stable seed replay.
#![forbid(unsafe_code)]
use openbindings::{
    Conformance, Document, ExternalSchemas, JsonLimits, JsonValue, PreparationLimits,
    PreparedSchema, ValueOutcome,
};
use openbindings_internal_json::decimal::Decimal;
use openbindings_json_schema_evaluator::{DefaultEvaluator, DefaultValidator};
use std::cmp::Ordering;

/// Keep individual fuzz iterations small; production admission limits still apply.
pub const MAX_INPUT: usize = 16 * 1024;

/// Exercise invalid bytes too, and compare owned and borrowed document paths.
pub fn document(data: &[u8]) -> bool {
    if data.len() > MAX_INPUT {
        return false;
    }
    let limits = JsonLimits::default();
    let assessed = openbindings::validate_document_with_limits(data, limits);
    let owned = openbindings::validate_document_owned_with_limits(data.to_vec(), limits);
    match (&assessed, &owned) {
        (Ok(a), Ok(b)) => {
            assert_eq!(
                serde_json::to_value(a.report()).unwrap(),
                serde_json::to_value(b.report()).unwrap()
            );
            assert_eq!(
                a.conformant().is_some(),
                a.report().conclusion == Conformance::Conformant
            );
        }
        (Err(a), Err(b)) => assert_eq!(a, b),
        _ => panic!("owned and borrowed document validation disagree"),
    }
    match (Document::parse(data), Document::parse_owned(data.to_vec())) {
        (Ok(a), Ok(b)) => {
            assert_eq!(a.as_bytes(), b.as_bytes());
            let serialized = serde_json::to_vec(a.value()).unwrap();
            let reparsed = Document::parse(&serialized).unwrap();
            assert_eq!(
                a.value().semantic_eq(reparsed.value()),
                if a.value().has_duplicate_names() {
                    None
                } else {
                    Some(true)
                }
            );
            match (a.validate(), reparsed.validate()) {
                (Ok(a), Ok(b)) => assert_eq!(a.report().conclusion, b.report().conclusion),
                (Err(a), Err(b)) => assert_eq!(a, b),
                _ => panic!("document round trip changed version admission"),
            }
            true
        }
        (Err(a), Err(b)) => {
            assert_eq!(a, b);
            false
        }
        _ => panic!("owned and borrowed document parsing disagree"),
    }
}

/// Exact numbers, duplicate members and unpaired UTF-16 units must survive serialization.
pub fn json_value(data: &[u8]) -> bool {
    if data.len() > MAX_INPUT {
        return false;
    }
    match (
        JsonValue::parse(data),
        JsonValue::parse_owned_with_limits(data.to_vec(), JsonLimits::default()),
    ) {
        (Ok(value), Ok(owned)) => {
            assert_eq!(value.text(), owned.text());
            let serialized = serde_json::to_vec(&value).unwrap();
            let again = JsonValue::parse(&serialized).unwrap();
            assert_eq!(again.text(), value.text());
            assert_eq!(
                value.semantic_eq(&again),
                if value.has_duplicate_names() {
                    None
                } else {
                    Some(true)
                }
            );
            true
        }
        (Err(a), Err(b)) => {
            assert_eq!(a, b);
            false
        }
        _ => panic!("owned and borrowed JSON parsing disagree"),
    }
}

fn equivalent_number(token: &str) -> String {
    let split = token.find(['e', 'E']).unwrap_or(token.len());
    let (mantissa, exponent) = token.split_at(split);
    format!(
        "{mantissa}{}{exponent}",
        if mantissa.contains('.') { "0" } else { ".0" }
    )
}
fn negate(token: &str) -> String {
    token
        .strip_prefix('-')
        .map_or_else(|| format!("-{token}"), str::to_owned)
}

/// Input is [number, divisor, optional third number]. Decimal requires valid JSON tokens.
pub fn decimal(data: &[u8]) -> bool {
    if data.len() > MAX_INPUT {
        return false;
    }
    let Ok(input) = JsonValue::parse(data) else {
        return false;
    };
    let Some(a) = input.view().element(0).and_then(|v| v.number_text()) else {
        return false;
    };
    let Some(b) = input.view().element(1).and_then(|v| v.number_text()) else {
        return false;
    };
    let (x, y) = (Decimal::parse(a), Decimal::parse(b));
    let (xx, yy) = (
        Decimal::parse(&equivalent_number(a)),
        Decimal::parse(&equivalent_number(b)),
    );
    assert_eq!(x, xx);
    assert_eq!(y, yy);
    assert_eq!(x.compare(&y), y.compare(&x).reverse());
    assert_eq!(x.compare(&y) == Ordering::Equal, x == y);
    assert_eq!(x.compare(&y), xx.compare(&yy));
    if let (Ok(a), Ok(b)) = (a.parse::<i64>(), b.parse::<i64>()) {
        assert_eq!(x.compare(&y), a.cmp(&b));
    }
    assert_eq!(
        Decimal::parse(&negate(a)).compare(&Decimal::parse(&negate(b))),
        x.compare(&y).reverse()
    );
    if let Some(c) = input.view().element(2).and_then(|v| v.number_text()) {
        let z = Decimal::parse(c);
        if x.compare(&y) != Ordering::Greater && y.compare(&z) != Ordering::Greater {
            assert_ne!(x.compare(&z), Ordering::Greater);
        }
    }
    if !y.is_zero() {
        let divisor = Decimal::parse(b.trim_start_matches('-'));
        assert_eq!(x.multiple_of(&divisor), xx.multiple_of(&divisor));
        assert_eq!(
            x.multiple_of(&divisor),
            Decimal::parse(&negate(a)).multiple_of(&divisor)
        );
        assert_eq!(Decimal::parse("-0").multiple_of(&divisor), Some(true));
        assert_ne!(divisor.multiple_of(&divisor), Some(false));
        // Independent integer arithmetic oracle for the ordinary integer subset.
        if let (Ok(a), Ok(b)) = (a.parse::<i64>(), b.parse::<i64>()) {
            assert_eq!(
                x.multiple_of(&divisor),
                Some(i128::from(a) % i128::from(b) == 0)
            );
        }
    }
    true
}

fn prepare(schema: &str) -> Option<PreparedSchema<DefaultValidator>> {
    let resources = ExternalSchemas::default()
        .with("https://fuzz.invalid/root", JsonValue::parse(schema).ok()?)
        .ok()?;
    let document = Document::parse(br#"{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://fuzz.invalid/root"}}}}"#).unwrap();
    document
        .validate()
        .unwrap()
        .conformant()
        .unwrap()
        .resolve_operation("op")
        .unwrap()
        .input()
        .unwrap()
        .prepare(
            &DefaultEvaluator::new(),
            &resources,
            &PreparationLimits::default(),
        )
        .ok()
}
fn verdict(outcome: &ValueOutcome) -> Option<bool> {
    match outcome {
        ValueOutcome::Satisfies => Some(true),
        ValueOutcome::Fails { .. } => Some(false),
        ValueOutcome::Undefined { .. } | ValueOutcome::NoVerdict { .. } => None,
    }
}
fn compare(
    prepared: &PreparedSchema<DefaultValidator>,
    value: &JsonValue,
    schema: &str,
) -> Option<bool> {
    let evaluated = prepared.evaluate(value);
    let explained = prepared.explain(value);
    match (verdict(&evaluated), verdict(&explained)) {
        (Some(a), Some(b)) => {
            assert_eq!(
                a,
                b,
                "schema={schema} value={} evaluate={evaluated:?} explain={explained:?}",
                value.text()
            );
            Some(a)
        }
        _ => None,
    }
}

/// Input is [schema, value], preserving exact JSON tokens through preparation.
pub fn lanes(data: &[u8]) -> bool {
    if data.len() > MAX_INPUT {
        return false;
    }
    let Ok(input) = JsonValue::parse(data) else {
        return false;
    };
    let (Some(schema), Some(value)) = (input.view().element(0), input.view().element(1)) else {
        return false;
    };
    let Some(prepared) = prepare(schema.text()) else {
        return false;
    };
    compare(&prepared, &value.to_owned(), schema.text()).is_some()
}

/// Input is [ECMAScript pattern, string]. Raw UTF-8 also reaches the reader as a pattern.
/// Preparation reaches the private reader without adding a production API.
pub fn pattern(data: &[u8]) -> bool {
    if data.len() > MAX_INPUT {
        return false;
    }
    let input = JsonValue::parse(data).ok();
    let pair = input.as_ref().and_then(|v| {
        Some((
            v.view().element(0)?.as_str()?,
            v.view().element(1)?.as_str()?,
        ))
    });
    let (pattern, text) = match pair {
        Some(pair) => pair,
        None => match std::str::from_utf8(data) {
            Ok(pattern) => (pattern, "a😀\n"),
            Err(_) => return false,
        },
    };
    let schema = serde_json::json!({"type":"string", "pattern":pattern}).to_string();
    let names =
        serde_json::json!({"type":"object", "propertyNames":{"pattern":pattern}}).to_string();
    if let (Some(string), Some(object)) = (prepare(&schema), prepare(&names)) {
        let a = compare(&string, &JsonValue::string(text).unwrap(), &schema);
        let value = JsonValue::parse(serde_json::json!({text: null}).to_string()).unwrap();
        let b = compare(&object, &value, &names);
        if let (Some(a), Some(b)) = (a, b) {
            assert_eq!(
                a, b,
                "pattern and propertyNames differ: pattern={pattern:?} text={text:?}"
            );
            return true;
        }
    }
    false
}
