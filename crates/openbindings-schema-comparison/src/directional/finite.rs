//! The profile's literal inclusion proof. Refusal here is no counterexample.
use super::*;

pub(super) fn inclusion(
    flow: &Object,
    receiver: &Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    let source = if let Some(value) = get(flow, "const") {
        if get(flow, "enum").is_some()
            && !Membership::new(array(flow, "enum"), 1, depth + 1, attempt)?.contains(
                value,
                depth + 1,
                attempt,
            )?
        {
            return Ok(true);
        }
        std::slice::from_ref(value)
    } else {
        array(flow, "enum")
    };
    // Prepare root membership once, retaining n log n work for finite sets.
    let checker = Literal::new(receiver, source.len(), depth, attempt)?;
    for value in source {
        if !attempt.scratch(|attempt| checker.accepts(value, depth, attempt))? {
            return Ok(false);
        }
    }
    Ok(true)
}

struct Literal<'a> {
    map: Option<&'a Object>,
    membership: Option<Membership<'a>>,
    only_finite: bool,
}
impl<'a> Literal<'a> {
    fn new(
        schema: &'a Value,
        probes: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<Self, Failure> {
        attempt.depth(depth)?;
        let map = match schema.view() {
            Value::Object(map)
                if get(map, "oneOf").is_none()
                    && get(map, "allOf").is_none()
                    && !outside(map, depth, attempt)? =>
            {
                Some(map)
            }
            _ => None,
        };
        let membership = if let Some(map) = map.filter(|map| get(map, "enum").is_some()) {
            Some(Membership::new(
                array(map, "enum"),
                probes,
                depth + 1,
                attempt,
            )?)
        } else {
            None
        };
        let only_finite = map.is_some_and(|map| {
            constraints(map).all(|(key, _)| is(key, "const") || is(key, "enum"))
        });
        Ok(Self {
            map,
            membership,
            only_finite,
        })
    }

    fn accepts(
        &self,
        value: &Value,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<bool, Failure> {
        attempt.depth(depth)?;
        let Some(map) = self.map else {
            return Ok(false);
        };
        if let Some(constant) = get(map, "const")
            && !value::equal(value, constant, depth + 1, attempt)?
        {
            return Ok(false);
        }
        if let Some(membership) = &self.membership
            && !membership.contains(value, depth + 1, attempt)?
        {
            return Ok(false);
        }
        if self.only_finite {
            return Ok(true);
        }
        if !typed_value(map, value, attempt)? {
            return Ok(false);
        }
        let accepted = match value.view() {
            Value::Number(number) => numeric(map, number, attempt)?,
            Value::String(units) => {
                attempt.work(units.len().saturating_add(1))?;
                length(
                    map,
                    "minLength",
                    "maxLength",
                    char::decode_utf16(units.iter().copied()).count(),
                    attempt,
                )?
            }
            Value::Array(values) => {
                if !length(map, "minItems", "maxItems", values.len(), attempt)? {
                    return Ok(false);
                }
                if let Some(items) = get(map, "items") {
                    let checker = Self::new(items, values.len(), depth + 1, attempt)?;
                    for value in values {
                        if !checker.accepts(value, depth + 1, attempt)? {
                            return Ok(false);
                        }
                    }
                }
                true
            }
            Value::Object(values) => object_members(map, values, depth, attempt)?,
            _ => true,
        };
        if !accepted {
            return Ok(false);
        }
        if let Some(Value::Union(variants)) = get(map, "anyOf") {
            for variant in variants {
                if Self::new(variant, 1, depth + 1, attempt)?.accepts(value, depth + 1, attempt)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        Ok(true)
    }
}

fn numeric(
    map: &Object,
    number: &openbindings::JsonValue,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    for (key, refused, exclusive) in [
        ("minimum", Ordering::Less, false),
        ("exclusiveMinimum", Ordering::Less, true),
        ("maximum", Ordering::Greater, false),
        ("exclusiveMaximum", Ordering::Greater, true),
    ] {
        if let Some(Value::Number(bound)) = get(map, key) {
            let order = value::numbers(number, bound, attempt)?;
            if order == refused || (exclusive && order == Ordering::Equal) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn length(
    map: &Object,
    min: &str,
    max: &str,
    count: usize,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    if get(map, min).is_none() && get(map, max).is_none() {
        return Ok(true);
    }
    // A machine-sized container length is exactly representable as JSON decimal.
    attempt.allocate(1024)?;
    let count = openbindings::JsonValue::parse(count.to_string())
        .map_err(|_| FailureKind::InvalidSchema)?;
    for (key, refused) in [(min, Ordering::Less), (max, Ordering::Greater)] {
        if let Some(Value::Number(bound)) = get(map, key)
            && value::numbers(&count, bound, attempt)? == refused
        {
            return Ok(false);
        }
    }
    Ok(true)
}
fn lookup<'a>(
    map: &'a Object,
    name: &[u16],
    attempt: &mut Attempt<'_>,
) -> Result<Option<&'a Value>, Failure> {
    attempt.work(
        name.len()
            .saturating_add(1)
            .saturating_mul(12)
            .saturating_mul(map.len().saturating_add(1).ilog2() as usize + 1),
    )?;
    Ok(map.get(name))
}
fn object_members(
    map: &Object,
    values: &Object,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    for required in array(map, "required") {
        let Value::String(name) = required else {
            return Err(FailureKind::InvalidSchema.into());
        };
        if lookup(values, name, attempt)?.is_none() {
            return Ok(false);
        }
    }
    let additional = get(map, "additionalProperties")
        .map(|schema| Literal::new(schema, values.len(), depth + 1, attempt))
        .transpose()?;
    for (name, value) in values {
        let property = properties(map)
            .map(|properties| lookup(properties, name, attempt))
            .transpose()?
            .flatten();
        let accepted = if let Some(schema) = property {
            Literal::new(schema, 1, depth + 1, attempt)?.accepts(value, depth + 1, attempt)?
        } else if let Some(checker) = &additional {
            checker.accepts(value, depth + 1, attempt)?
        } else {
            true
        };
        if !accepted {
            return Ok(false);
        }
    }
    Ok(true)
}

// Failure eligibility is judged against each authored value, not the position's
// nominal type set. A constraining applicable keyword still withholds failure.
pub(super) fn eligible(
    flow: &Object,
    value: &Value,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    for (key, constraint) in constraints(flow) {
        attempt.work(512)?;
        if ["type", "const", "enum"].iter().any(|k| is(key, k)) {
            continue;
        }
        let applies = if ["minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"]
            .iter()
            .any(|k| is(key, k))
        {
            matches!(value.view(), Value::Number(_))
        } else if ["minLength", "maxLength"].iter().any(|k| is(key, k)) {
            matches!(value.view(), Value::String(_))
        } else if ["items", "minItems", "maxItems"].iter().any(|k| is(key, k)) {
            matches!(value.view(), Value::Array(_))
        } else if ["properties", "required", "additionalProperties"]
            .iter()
            .any(|k| is(key, k))
        {
            matches!(value.view(), Value::Object(_))
        } else {
            true
        };
        if applies && !crate::normalize::top_keyword(key, constraint, depth, attempt)? {
            return Ok(false);
        }
    }
    Ok(true)
}
