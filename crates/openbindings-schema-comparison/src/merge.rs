use crate::ComparisonFailureKind as FailureKind;
use crate::{
    ComparisonFailure as Failure,
    budget::Attempt,
    normalize::{get_value as get, is, outside},
    value::{self, Name, Object, Value},
};
use std::{cmp::Ordering, collections::BTreeMap};

pub(crate) fn all_of(
    siblings: Object,
    branches: Vec<Value>,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Failure> {
    empty_on_conflict(all_of_inner(siblings, branches, depth, attempt), attempt)
}
// A merge either completes its position, finds the position empty (an empty
// type or enum intersection, or conflicting const values), or fails.
enum Stop {
    Empty,
    Failed(Failure),
}
impl From<Failure> for Stop {
    fn from(failure: Failure) -> Self {
        Self::Failed(failure)
    }
}
impl From<FailureKind> for Stop {
    fn from(kind: FailureKind) -> Self {
        Self::Failed(kind.into())
    }
}
// An empty position normalizes to the exact empty schema where it occurs.
fn empty_on_conflict(
    result: Result<Value, Stop>,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Failure> {
    match result {
        Ok(value) => Ok(value),
        Err(Stop::Empty) => crate::normalize::boolean_schema(false, attempt),
        Err(Stop::Failed(failure)) => Err(failure),
    }
}
fn all_of_inner(
    mut siblings: Object,
    mut branches: Vec<Value>,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Stop> {
    attempt.depth(depth)?;
    if siblings.is_empty() && branches.len() == 1 {
        let mut branch = branches
            .pop()
            .expect("sole branch")
            .owned(depth + 1, attempt)?;
        if let Value::Object(map) = &mut branch {
            deduplicate_enum(map, depth + 1, attempt)?;
        }
        return Ok(branch);
    }
    let mut retain = outside(&siblings, depth, attempt)? || unmergeable(&siblings);
    let mut closed_count = usize::from(constrains_additional(&siblings));
    let mut properties_count = usize::from(has_properties(&siblings));
    for branch in &branches {
        let Value::Object(object) = branch.view() else {
            retain = true;
            break;
        };
        if retain {
            break;
        }
        retain = outside(object, depth + 1, attempt)? || unmergeable(object);
        closed_count += usize::from(constrains_additional(object));
        properties_count += usize::from(has_properties(object));
    }
    if !retain && closed_count != 0 && properties_count != 0 {
        let mut closed = attempt.vector(closed_count)?;
        let mut properties = attempt.vector(properties_count)?;
        for object in std::iter::once(&siblings).chain(branches.iter().map(|branch| {
            let Value::Object(object) = branch.view() else {
                unreachable!("normalized branches are objects")
            };
            object
        })) {
            attempt.work(1)?;
            if constrains_additional(object) {
                closed.push(object);
            }
            if has_properties(object) {
                properties.push(object);
            }
        }
        'pairs: for object in closed {
            for other in &properties {
                if std::ptr::eq(object, *other) {
                    attempt.work(1)?;
                    continue;
                }
                if closed_across(object, other, attempt)? {
                    retain = true;
                    break 'pairs;
                }
            }
        }
    }
    if retain {
        let key = value::literal("allOf", attempt)?;
        value::insert(&mut siblings, key, Value::Array(branches), attempt)?;
        return Ok(Value::Object(siblings));
    }
    // A singleton still performs the enum intersection step, before unwrapping.
    deduplicate_enum(&mut siblings, depth, attempt)?;
    for branch in &mut branches {
        if let Value::Object(map) = branch.view()
            && get(map, "enum").is_some()
        {
            let mut owned = std::mem::replace(branch, Value::Object(Object::new()))
                .owned(depth + 1, attempt)?;
            if let Value::Object(map) = &mut owned {
                deduplicate_enum(map, depth + 1, attempt)?;
            }
            *branch = owned;
        }
    }
    let mut result = Value::Object(siblings);
    for branch in branches {
        result = pair(result, branch, depth + 1, attempt)?;
        if crate::normalize::empty_member(&result) {
            break;
        }
    }
    Ok(result)
}
fn constrains_additional(object: &Object) -> bool {
    matches!(get(object, "additionalProperties"), Some(Value::Object(map)) if !map.is_empty())
}
fn has_properties(object: &Object) -> bool {
    matches!(get(object, "properties"), Some(Value::Object(properties)) if !properties.is_empty())
}
// A branch's additionalProperties applies to properties absent from that branch,
// including properties declared in another branch. Merging must not admit them.
fn closed_across(
    closed: &Object,
    other: &Object,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    attempt.work(1)?;
    if !constrains_additional(closed) {
        return Ok(false);
    }
    let Some(Value::Object(properties)) = get(other, "properties") else {
        return Ok(false);
    };
    let declared = get(closed, "properties");
    for name in properties.keys() {
        attempt.work(
            name.len().saturating_add(1).saturating_mul(
                closed
                    .len()
                    .saturating_add(properties.len())
                    .saturating_add(1),
            ),
        )?;
        if !matches!(declared, Some(Value::Object(own)) if own.contains_key(name)) {
            return Ok(true);
        }
    }
    Ok(false)
}
fn unmergeable(object: &Object) -> bool {
    // A retained nested conjunction has no merge rule either. Keep its parent
    // rather than attempting to merge two retained allOf arrays as keywords.
    get(object, "oneOf").is_some()
        || get(object, "anyOf").is_some()
        || get(object, "allOf").is_some()
}
fn pair(
    left: Value,
    right: Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Failure> {
    attempt.depth(depth)?;
    let (Value::Object(left), Value::Object(right)) =
        (left.owned(depth, attempt)?, right.owned(depth, attempt)?)
    else {
        return Err(Failure::from(FailureKind::InvalidSchema));
    };
    if outside(&left, depth, attempt)?
        || outside(&right, depth, attempt)?
        || unmergeable(&left)
        || unmergeable(&right)
        || closed_across(&left, &right, attempt)?
        || closed_across(&right, &left, attempt)?
    {
        let mut branches = attempt.vector(2)?;
        branches.extend([Value::Object(left), Value::Object(right)]);
        return all_of(Object::new(), branches, depth, attempt);
    }
    empty_on_conflict(
        merge_maps(left, right, depth, attempt, false).map(Value::Object),
        attempt,
    )
}
fn merge_maps(
    left: Object,
    right: Object,
    depth: usize,
    attempt: &mut Attempt<'_>,
    properties: bool,
) -> Result<Object, Stop> {
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
) -> Result<Value, Stop> {
    attempt.depth(depth)?;
    if is(key, "properties") {
        let (Value::Object(left), Value::Object(right)) =
            (left.owned(depth, attempt)?, right.owned(depth, attempt)?)
        else {
            return Err(FailureKind::InvalidSchema.into());
        };
        return merge_maps(left, right, depth, attempt, true).map(Value::Object);
    }
    if is(key, "items") {
        return Ok(pair(left, right, depth, attempt)?);
    }
    if is(key, "additionalProperties") {
        if crate::normalize::empty_member(&left) {
            return Ok(left);
        }
        if crate::normalize::empty_member(&right) {
            return Ok(right);
        }
        return Ok(pair(left, right, depth, attempt)?);
    }
    if is(key, "const") {
        return if value::equal(&left, &right, depth, attempt)? {
            Ok(left)
        } else {
            Err(Stop::Empty)
        };
    }
    if is(key, "type") || is(key, "required") {
        return strings(left, right, is(key, "type"), attempt);
    }
    if is(key, "enum") {
        let (Value::Array(left), Value::Array(right)) = (left, right) else {
            return Err(FailureKind::InvalidSchema.into());
        };
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
            Err(Stop::Empty)
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
            return Err(FailureKind::InvalidSchema.into());
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
    Err(FailureKind::UnsupportedNormalization.into())
}
fn strings(
    left: Value,
    right: Value,
    intersection: bool,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Stop> {
    let (Value::Array(left), Value::Array(right)) = (left, right) else {
        return Err(FailureKind::InvalidSchema.into());
    };
    let mut names = BTreeMap::<Name, ()>::new();
    if intersection {
        for a in left {
            let Value::String(a) = a else {
                return Err(FailureKind::InvalidSchema.into());
            };
            for b in &right {
                let Value::String(b) = b else {
                    return Err(FailureKind::InvalidSchema.into());
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
            return Err(Stop::Empty);
        }
        // An intersection containing number already includes its integer
        // subtype. The pinned typeIntersection fixture requires this reduction
        // in a merge; standalone canonical type arrays only sort/deduplicate.
        if crate::normalize::get(&names, "number").is_some() {
            let integer = value::literal("integer", attempt)?;
            names.remove(integer.as_slice());
        }
    } else {
        for name in left.into_iter().chain(right) {
            let Value::String(name) = name else {
                return Err(FailureKind::InvalidSchema.into());
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

fn deduplicate_enum(map: &mut Object, depth: usize, attempt: &mut Attempt<'_>) -> Result<(), Stop> {
    let key = value::literal("enum", attempt)?;
    let Some(Value::Array(values)) = map.remove(&key) else {
        return Ok(());
    };
    let mut unique = attempt.vector(values.len())?;
    for value in values {
        let mut repeated = false;
        for previous in &unique {
            if value::equal(&value, previous, depth + 1, attempt)? {
                repeated = true;
                break;
            }
        }
        if !repeated {
            unique.push(value);
        }
    }
    if unique.is_empty() {
        return Err(Stop::Empty);
    }
    Ok(value::insert(map, key, Value::Array(unique), attempt)?)
}
