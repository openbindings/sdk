use crate::{
    ComparisonFailure as Failure,
    budget::Attempt,
    normalize::{get, is, outside},
    value::{self, Name, Object, Value},
};
use std::{cmp::Ordering, collections::BTreeMap};

pub(crate) fn all_of(
    mut siblings: Object,
    branches: Vec<Value>,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Failure> {
    attempt.depth(depth)?;
    let mut retain = outside(&siblings, depth, attempt)?;
    for branch in &branches {
        let Value::Object(object) = branch else {
            return Err(Failure::InvalidSchema);
        };
        retain |= outside(object, depth + 1, attempt)?;
    }
    if retain {
        let key = value::literal("allOf", attempt)?;
        value::insert(&mut siblings, key, Value::Array(branches), attempt)?;
        return Ok(Value::Object(siblings));
    }
    // Refuse the entire merge after all branches normalized; no earlier false
    // flag or empty intersection conceals a required reference/capability error.
    if unions(&siblings)
        || branches
            .iter()
            .any(|branch| matches!(branch,Value::Object(object) if unions(object)))
    {
        return Err(Failure::UnionMerge);
    }
    let mut result = Value::Object(siblings);
    for branch in branches {
        result = pair(result, branch, depth + 1, attempt)?;
    }
    Ok(result)
}
fn unions(object: &Object) -> bool {
    get(object, "oneOf").is_some() || get(object, "anyOf").is_some()
}
fn pair(
    left: Value,
    right: Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Failure> {
    attempt.depth(depth)?;
    let (Value::Object(left), Value::Object(right)) = (left, right) else {
        return Err(Failure::InvalidSchema);
    };
    if outside(&left, depth, attempt)? || outside(&right, depth, attempt)? {
        let mut branches = attempt.vector(2)?;
        branches.extend([Value::Object(left), Value::Object(right)]);
        return all_of(Object::new(), branches, depth, attempt);
    }
    if unions(&left) || unions(&right) {
        return Err(Failure::UnionMerge);
    }
    merge_maps(left, right, depth, attempt, false).map(Value::Object)
}
fn merge_maps(
    left: Object,
    right: Object,
    depth: usize,
    attempt: &mut Attempt<'_>,
    properties: bool,
) -> Result<Object, Failure> {
    attempt.depth(depth)?;
    let mut result = Object::new();
    let mut left = left.into_iter().peekable();
    let mut right = right.into_iter().peekable();
    while left.peek().is_some() || right.peek().is_some() {
        attempt.work(1)?;
        let order = match (left.peek(), right.peek()) {
            (Some((a, _)), Some((b, _))) => {
                attempt.work(a.len().min(b.len()).saturating_add(1))?;
                a.cmp(b)
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            _ => unreachable!(),
        };
        let (key, value) = match order {
            Ordering::Less => left.next().expect("left"),
            Ordering::Greater => right.next().expect("right"),
            Ordering::Equal => {
                let (key, a) = left.next().expect("left");
                let (_, b) = right.next().expect("right");
                let merged = if properties {
                    pair(a, b, depth + 1, attempt)?
                } else {
                    keyword(&key, a, b, depth + 1, attempt)?
                };
                (key, merged)
            }
        };
        value::insert(&mut result, key, value, attempt)?;
    }
    Ok(result)
}
fn keyword(
    key: &[u16],
    left: Value,
    right: Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Failure> {
    attempt.depth(depth)?;
    if is(key, "properties") {
        let (Value::Object(left), Value::Object(right)) = (left, right) else {
            return Err(Failure::InvalidSchema);
        };
        return merge_maps(left, right, depth, attempt, true).map(Value::Object);
    }
    if is(key, "items") {
        return pair(left, right, depth, attempt);
    }
    if is(key, "additionalProperties") {
        return match (left, right) {
            (Value::Boolean(false), _) | (_, Value::Boolean(false)) => Ok(Value::Boolean(false)),
            (Value::Boolean(true), other) | (other, Value::Boolean(true)) => Ok(other),
            (left, right) => pair(left, right, depth, attempt),
        };
    }
    if is(key, "const") {
        return if value::equal(&left, &right, depth, attempt)? {
            Ok(left)
        } else {
            Err(Failure::UnsatisfiableMerge)
        };
    }
    if is(key, "type") || is(key, "required") {
        return strings(left, right, is(key, "type"), attempt);
    }
    if is(key, "enum") {
        let (Value::Array(left), Value::Array(right)) = (left, right) else {
            return Err(Failure::InvalidSchema);
        };
        // The profile specifies first-branch order, but no canonical treatment
        // of repeated equal first-branch values in an intersection.
        for (index, value) in left.iter().enumerate() {
            for previous in &left[..index] {
                if value::equal(value, previous, depth + 1, attempt)? {
                    return Err(Failure::UnsupportedNormalization);
                }
            }
        }
        let mut result = attempt.vector(left.len())?;
        for value in left {
            let mut admitted = false;
            for candidate in &right {
                if value::equal(&value, candidate, depth + 1, attempt)? {
                    admitted = true;
                    break;
                }
            }
            if admitted {
                result.push(value);
            }
        }
        return if result.is_empty() {
            Err(Failure::UnsatisfiableMerge)
        } else {
            Ok(Value::Array(result))
        };
    }
    let higher = ["minimum", "exclusiveMinimum", "minLength", "minItems"]
        .iter()
        .any(|name| is(key, name));
    let lower = ["maximum", "exclusiveMaximum", "maxLength", "maxItems"]
        .iter()
        .any(|name| is(key, name));
    if higher || lower {
        let (Value::Number(a), Value::Number(b)) = (&left, &right) else {
            return Err(Failure::InvalidSchema);
        };
        let order = value::numbers(a, b, attempt)?;
        return Ok(
            if (higher && order == Ordering::Less) || (lower && order == Ordering::Greater) {
                right
            } else {
                left
            },
        );
    }
    Err(Failure::UnsupportedNormalization)
}
fn strings(
    left: Value,
    right: Value,
    intersection: bool,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Failure> {
    let (Value::Array(left), Value::Array(right)) = (left, right) else {
        return Err(Failure::InvalidSchema);
    };
    let mut names = BTreeMap::<Name, ()>::new();
    if intersection {
        for a in left {
            let Value::String(a) = a else {
                return Err(Failure::InvalidSchema);
            };
            for b in &right {
                let Value::String(b) = b else {
                    return Err(Failure::InvalidSchema);
                };
                attempt.work(a.len().min(b.len()).saturating_add(1))?;
                let name = if a == *b {
                    Some(a.as_slice())
                } else if is(&a, "number") && is(b, "integer") {
                    Some(b.as_slice())
                } else if is(&a, "integer") && is(b, "number") {
                    Some(a.as_slice())
                } else {
                    None
                };
                if let Some(name) = name {
                    let mut owned = attempt.vector(name.len())?;
                    owned.extend_from_slice(name);
                    put_string(&mut names, owned, attempt)?;
                }
            }
        }
        if names.is_empty() {
            return Err(Failure::UnsatisfiableMerge);
        }
        // An intersection containing number already includes its integer
        // subtype. The pinned typeIntersection fixture requires this reduction
        // in a merge; standalone canonical type arrays only sort/deduplicate.
        if get(&names, "number").is_some() {
            let integer = value::literal("integer", attempt)?;
            names.remove(integer.as_slice());
        }
    } else {
        for name in left.into_iter().chain(right) {
            let Value::String(name) = name else {
                return Err(Failure::InvalidSchema);
            };
            put_string(&mut names, name, attempt)?;
        }
    }
    let mut values = attempt.vector(names.len())?;
    values.extend(names.into_keys().map(Value::String));
    Ok(Value::Array(values))
}
fn put_string(
    names: &mut BTreeMap<Name, ()>,
    name: Name,
    attempt: &mut Attempt<'_>,
) -> Result<(), Failure> {
    attempt.work(
        name.len()
            .saturating_add(1)
            .saturating_mul(12)
            .saturating_mul(names.len().saturating_add(1).ilog2() as usize + 1),
    )?;
    if !names.contains_key(&name) {
        value::insert(names, name, (), attempt)?;
    }
    Ok(())
}
