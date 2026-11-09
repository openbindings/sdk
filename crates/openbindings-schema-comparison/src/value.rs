//! Private exact values. Only normalization creates Union at a schema keyword;
//! opaque instance arrays remain ordered even when their member is named anyOf.
use crate::{ComparisonFailure as Failure, budget::Attempt};
use openbindings::{JsonKind, JsonRef, JsonValue};
use openbindings_internal_json::decimal::Decimal;
use std::{cmp::Ordering, collections::BTreeMap};

pub(crate) type Name = Vec<u16>;
pub(crate) type Object = BTreeMap<Name, Value>;
pub(crate) enum Value {
    Null,
    Boolean(bool),
    Number(JsonValue),
    String(Name),
    Array(Vec<Value>),
    Object(Object),
    Union(Vec<Value>),
}

pub(crate) fn name(value: JsonRef<'_>, attempt: &mut Attempt<'_>) -> Result<Name, Failure> {
    let string = value.as_string().ok_or(Failure::InvalidSchema)?;
    let bound = value.text().len();
    attempt.work(bound.saturating_mul(4).saturating_add(1))?;
    let mut units = attempt.vector::<u16>(bound)?;
    units.extend(string.utf16_units());
    Ok(units)
}
pub(crate) fn literal(text: &str, attempt: &mut Attempt<'_>) -> Result<Name, Failure> {
    attempt.work(text.len().saturating_add(1))?;
    let mut units = attempt.vector::<u16>(text.len())?;
    units.extend(text.encode_utf16());
    Ok(units)
}

/// Reserve ordered-map search work and an amortized cumulative allocation bound.
/// A BTreeMap node has at most 11 key/value slots and 12 child pointers. Two
/// maximum nodes per insertion bound all cumulative splits of an insertion-only
/// map, including a new root; values/strings are separately reserved by owners.
pub(crate) fn insert<T>(
    object: &mut BTreeMap<Name, T>,
    key: Name,
    value: T,
    attempt: &mut Attempt<'_>,
) -> Result<(), Failure> {
    let height = object.len().saturating_add(1).ilog2() as usize + 1;
    attempt.work(
        12usize
            .saturating_mul(height)
            .saturating_mul(key.len().saturating_add(1)),
    )?;
    attempt.allocate(2 * (12 * std::mem::size_of::<(Name, T)>() + 256))?;
    if object.insert(key, value).is_some() {
        Err(Failure::DuplicateMember)
    } else {
        Ok(())
    }
}

pub(crate) fn import(
    value: JsonRef<'_>,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Failure> {
    attempt.depth(depth)?;
    Ok(match value.kind() {
        JsonKind::Null => Value::Null,
        JsonKind::Boolean => Value::Boolean(value.as_bool().expect("boolean")),
        JsonKind::Number => Value::Number(value.to_owned()),
        JsonKind::String => Value::String(name(value, attempt)?),
        JsonKind::Array => {
            let elements = value.elements().expect("array");
            let mut items = attempt.vector(elements.len())?;
            for child in elements {
                items.push(import(child, depth + 1, attempt)?);
            }
            Value::Array(items)
        }
        JsonKind::Object => {
            let mut object = Object::new();
            for member in value.members().expect("object") {
                let key = name(member.name, attempt)?;
                let child = import(member.value, depth + 1, attempt)?;
                insert(&mut object, key, child, attempt)?;
            }
            Value::Object(object)
        }
    })
}

pub(crate) fn numbers(
    left: &JsonValue,
    right: &JsonValue,
    attempt: &mut Attempt<'_>,
) -> Result<Ordering, Failure> {
    let left = left.view().number_text().ok_or(Failure::InvalidSchema)?;
    let right = right.view().number_text().ok_or(Failure::InvalidSchema)?;
    let bytes = left.len().saturating_add(right.len());
    // Parsing and comparison retain significant digits and compact decimal
    // powers. Neither expands an exponent into the represented number's zeros.
    attempt.work(bytes.saturating_mul(16).saturating_add(1))?;
    attempt.allocate(bytes.saturating_mul(16).saturating_add(512))?;
    let order = Decimal::parse(left).compare(&Decimal::parse(right));
    attempt.check()?;
    Ok(order)
}

pub(crate) fn equal(
    left: &Value,
    right: &Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    attempt.depth(depth)?;
    Ok(match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => numbers(a, b, attempt)? == Ordering::Equal,
        (Value::String(a), Value::String(b)) => {
            attempt.work(a.len().min(b.len()).saturating_add(1))?;
            a == b
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (a, b) in a.iter().zip(b) {
                if !equal(a, b, depth + 1, attempt)? {
                    return Ok(false);
                }
            }
            true
        }
        (Value::Object(a), Value::Object(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for ((a_key, a), (b_key, b)) in a.iter().zip(b) {
                attempt.work(a_key.len().min(b_key.len()).saturating_add(1))?;
                if a_key != b_key || !equal(a, b, depth + 1, attempt)? {
                    return Ok(false);
                }
            }
            true
        }
        (Value::Union(a), Value::Union(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            let mut used = attempt.vector::<bool>(b.len())?;
            used.resize(b.len(), false);
            for left in a {
                let mut found = false;
                for (index, right) in b.iter().enumerate() {
                    attempt.work(1)?;
                    if !used[index] && equal(left, right, depth + 1, attempt)? {
                        used[index] = true;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Ok(false);
                }
            }
            true
        }
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComparisonLimits;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};

    fn compare(left: &str, right: &str) -> Result<bool, Failure> {
        let control = AtomicBool::new(false);
        let mut attempt = Attempt::new(ComparisonLimits::default(), &control);
        let left = JsonValue::parse(left).unwrap();
        let right = JsonValue::parse(right).unwrap();
        let left = import(left.view(), 0, &mut attempt)?;
        let right = import(right.view(), 0, &mut attempt)?;
        equal(&left, &right, 0, &mut attempt)
    }
    #[test]
    fn exact_values_distinguish_number_type_absence_unicode_and_authored_arrays() {
        for (a, b, expected) in [
            ("9007199254740993", "9007199254740992", false),
            ("1.0000000000000000001", "1", false),
            (
                "1e999999999999999999999999999",
                "10e999999999999999999999999998",
                true,
            ),
            ("-0", "0.0e-9999999999999999999999", true),
            (r#"{"a":1,"b":2}"#, r#"{"b":2e0,"a":1.00}"#, true),
            (r#"{"a":null}"#, "{}", false),
            ("1", r#""1""#, false),
            (r#""\ud800""#, r#""\uD800""#, true),
            (r#""\u00e9""#, r#""e\u0301""#, false),
            (r#"{"anyOf":[1,2]}"#, r#"{"anyOf":[2,1]}"#, false),
        ] {
            assert_eq!(compare(a, b), Ok(expected), "{a} / {b}");
        }
        assert_eq!(
            compare(r#"{"a":1,"\u0061":1}"#, "{}"),
            Err(Failure::DuplicateMember)
        );
    }
    #[test]
    fn schema_union_arrays_are_multisets_with_multiplicity() {
        let control = AtomicBool::new(false);
        let mut attempt = Attempt::new(ComparisonLimits::default(), &control);
        let variants =
            |values: &[bool]| Value::Union(values.iter().copied().map(Value::Boolean).collect());
        assert!(
            equal(
                &variants(&[true, false, true]),
                &variants(&[true, true, false]),
                0,
                &mut attempt
            )
            .unwrap()
        );
        assert!(
            !equal(
                &variants(&[true, false, true]),
                &variants(&[false, true, false]),
                0,
                &mut attempt
            )
            .unwrap()
        );
        assert!(
            !equal(
                &variants(&[true]),
                &Value::Array(vec![Value::Boolean(true)]),
                0,
                &mut attempt
            )
            .unwrap()
        );
    }
    #[test]
    fn resources_and_cancellation_refuse_without_comparison_verdict() {
        let value = JsonValue::parse(r#"{"long member":{"nested":[1,2,3]}}"#).unwrap();
        let control = AtomicBool::new(false);
        for limits in [
            ComparisonLimits {
                max_work: 1,
                ..Default::default()
            },
            ComparisonLimits {
                max_allocation_bytes: 1,
                ..Default::default()
            },
            ComparisonLimits {
                max_depth: 1,
                ..Default::default()
            },
        ] {
            assert!(matches!(
                import(value.view(), 0, &mut Attempt::new(limits, &control)),
                Err(Failure::LimitExceeded(_))
            ));
        }
        struct Control {
            count: AtomicUsize,
            cancel: usize,
        }
        impl openbindings::WorkControl for Control {
            fn is_cancelled(&self) -> bool {
                self.count.fetch_add(1, AtomicOrdering::Relaxed) >= self.cancel
            }
        }
        let baseline = Control {
            count: AtomicUsize::new(0),
            cancel: usize::MAX,
        };
        import(
            value.view(),
            0,
            &mut Attempt::new(ComparisonLimits::default(), &baseline),
        )
        .unwrap();
        for cancel in 0..baseline.count.load(AtomicOrdering::Relaxed) {
            let control = Control {
                count: AtomicUsize::new(0),
                cancel,
            };
            assert!(matches!(
                import(
                    value.view(),
                    0,
                    &mut Attempt::new(ComparisonLimits::default(), &control)
                ),
                Err(Failure::Cancelled)
            ));
        }
    }
}
