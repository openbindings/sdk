//! Ordered membership over borrowed exact instance values, without hashing or
//! serialization. Schema-only Reference/Union values never occur in these sets.
use super::{Value, equal, numbers};
use crate::{ComparisonFailure as Failure, ComparisonFailureKind, budget::Attempt};
use std::{borrow::Borrow, cmp::Ordering};

fn rank(value: &Value) -> Result<u8, Failure> {
    Ok(match value.view() {
        Value::Null => 0,
        Value::Boolean(_) => 1,
        Value::Number(_) => 2,
        Value::String(_) => 3,
        Value::Array(_) => 4,
        Value::Object(_) => 5,
        _ => return Err(ComparisonFailureKind::InvalidSchema.into()),
    })
}

// This order's Equal class is exactly value::equal for instance values. Type
// ranks and length-first container ordering are arbitrary; numbers use the same
// compact exact decimal comparison, strings/names the same decoded UTF-16 units.
fn order(
    left: &Value,
    right: &Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Ordering, Failure> {
    attempt.depth(depth)?;
    let kinds = rank(left)?.cmp(&rank(right)?);
    if kinds != Ordering::Equal {
        return Ok(kinds);
    }
    Ok(match (left.view(), right.view()) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Boolean(a), Value::Boolean(b)) => a.cmp(b),
        (Value::Number(a), Value::Number(b)) => {
            attempt.scratch(|attempt| numbers(a, b, attempt))?
        }
        (Value::String(a), Value::String(b)) => {
            attempt.work(a.len().min(b.len()).saturating_add(1))?;
            a.cmp(b)
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Ok(a.len().cmp(&b.len()));
            }
            for (a, b) in a.iter().zip(b) {
                let result = order(a, b, depth + 1, attempt)?;
                if result != Ordering::Equal {
                    return Ok(result);
                }
            }
            Ordering::Equal
        }
        (Value::Object(a), Value::Object(b)) => {
            if a.len() != b.len() {
                return Ok(a.len().cmp(&b.len()));
            }
            for ((ak, a), (bk, b)) in a.iter().zip(b) {
                attempt.work(ak.len().min(bk.len()).saturating_add(1))?;
                let names = ak.cmp(bk);
                if names != Ordering::Equal {
                    return Ok(names);
                }
                let result = order(a, b, depth + 1, attempt)?;
                if result != Ordering::Equal {
                    return Ok(result);
                }
            }
            Ordering::Equal
        }
        _ => unreachable!("equal instance kinds"),
    })
}

pub(crate) fn contains_sorted<T: Borrow<Value>>(
    values: &[T],
    value: &Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    if values.len() <= 8 {
        for other in values {
            if equal(value, other.borrow(), depth, attempt)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    let (mut lo, mut hi) = (0, values.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let other = values[mid].borrow();
        match order(value, other, depth, attempt)? {
            Ordering::Less => hi = mid,
            Ordering::Greater => lo = mid + 1,
            Ordering::Equal => return equal(value, other, depth, attempt),
        }
    }
    Ok(false)
}

pub(crate) struct Membership<'a> {
    values: &'a [Value],
    sorted: Option<Vec<&'a Value>>,
}
impl<'a> Membership<'a> {
    pub(crate) fn new(
        values: &'a [Value],
        probes: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<Self, Failure> {
        // One lookup is already linear, and bounded small sets need no index.
        let sorted = if probes > 1 && values.len() > 8 {
            let mut sorted = attempt.vector(values.len())?;
            sorted.extend(values);
            // Fallible heapsort keeps cancellation, depth and allocation refusals
            // out of an infallible Ord implementation. Worst case is n log n.
            for root in (0..sorted.len() / 2).rev() {
                sift(&mut sorted, root, depth, attempt)?;
            }
            for end in (1..sorted.len()).rev() {
                sorted.swap(0, end);
                sift(&mut sorted[..end], 0, depth, attempt)?;
            }
            Some(sorted)
        } else {
            None
        };
        Ok(Self { values, sorted })
    }
    pub(crate) fn contains(
        &self,
        value: &Value,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<bool, Failure> {
        if let Some(sorted) = &self.sorted {
            return contains_sorted(sorted, value, depth, attempt);
        }
        for other in self.values {
            if equal(value, other, depth, attempt)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
fn sift(
    values: &mut [&Value],
    mut root: usize,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<(), Failure> {
    while root < values.len() / 2 {
        let mut child = 2 * root + 1;
        if child + 1 < values.len()
            && order(values[child], values[child + 1], depth, attempt)? == Ordering::Less
        {
            child += 1;
        }
        if order(values[root], values[child], depth, attempt)? != Ordering::Less {
            break;
        }
        values.swap(root, child);
        root = child;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ComparisonLimits, value::import};
    use openbindings::JsonValue;
    use std::sync::atomic::AtomicBool;
    #[allow(dead_code)]
    mod generator {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/property_generator.rs"
        ));
    }

    #[test]
    fn generated_instance_order_agrees_with_exact_equality_and_membership() {
        let mut rng = generator::Rng(0x20261114);
        let mut texts = (0..256)
            .map(|_| generator::value(&mut rng, 3).to_string())
            .collect::<Vec<_>>();
        texts.extend(
            [
                "1",
                "1.0",
                "1e0",
                "-0",
                "0.00e-999999999999999999999",
                "9007199254740992",
                "9007199254740993",
                "1.0000000000000000001",
                "1e999999999999999999999",
                "10e999999999999999999998",
                "-1e-999999999999999999999",
                "-10e-1000000000000000000000",
                r#""\ud800""#,
                r#""\uD800""#,
                r#""\ud801""#,
                r#""\udc00""#,
                r#""é""#,
                r#""e\u0301""#,
                r#""😀""#,
                r#""\ud83d\ude00""#,
                r#"{"z":[1.0,"\ud800"],"a":null}"#,
                r#"{"a":null,"z":[1e0,"\uD800"]}"#,
                r#"{"anyOf":[1,2]}"#,
                r#"{"anyOf":[2,1]}"#,
            ]
            .map(str::to_owned),
        );
        let control = AtomicBool::new(false);
        let mut attempt = Attempt::new(ComparisonLimits::default(), &control);
        let values = texts
            .iter()
            .map(|text| import(JsonValue::parse(text).unwrap().view(), 0, &mut attempt).unwrap())
            .collect::<Vec<_>>();
        for a in &values {
            for b in &values {
                let ab = order(a, b, 0, &mut attempt).unwrap();
                assert_eq!(ab == Ordering::Equal, equal(a, b, 0, &mut attempt).unwrap());
                assert_eq!(ab, order(b, a, 0, &mut attempt).unwrap().reverse());
            }
        }
        let index = Membership::new(&values, values.len(), 0, &mut attempt).unwrap();
        let sorted = index.sorted.as_ref().unwrap();
        for (i, a) in sorted.iter().enumerate() {
            for b in &sorted[i..] {
                assert_ne!(order(a, b, 0, &mut attempt).unwrap(), Ordering::Greater);
            }
            assert!(index.contains(a, 0, &mut attempt).unwrap());
        }
        let subset = Membership::new(&values[..64], values.len(), 0, &mut attempt).unwrap();
        for a in &values {
            let expected = values[..64]
                .iter()
                .any(|b| equal(a, b, 0, &mut attempt).unwrap());
            assert_eq!(subset.contains(a, 0, &mut attempt).unwrap(), expected);
        }
    }
}
