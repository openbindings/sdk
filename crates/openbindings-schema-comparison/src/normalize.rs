use crate::{
    ComparisonFailure as Failure, SchemaInput,
    budget::Attempt,
    merge,
    reference::Resources,
    value::{self, Name, Object, Value},
};
use openbindings::{JsonKind, JsonRef};
use openbindings_internal_json::backend;
use std::collections::BTreeMap;

pub(crate) fn is(name: &[u16], text: &str) -> bool {
    name.iter().copied().eq(text.encode_utf16())
}
/// All callers use short ASCII profile keyword constants; no allocation or
/// Unicode conversion of arbitrary names occurs during keyword lookup.
pub(crate) fn get<'a, T>(map: &'a BTreeMap<Name, T>, text: &str) -> Option<&'a T> {
    let mut key = [0u16; 32];
    for (unit, byte) in key.iter_mut().zip(text.bytes()) {
        *unit = u16::from(byte);
    }
    map.get(&key[..text.len()])
}
pub(crate) fn known(name: &[u16]) -> bool {
    [
        "allOf",
        "oneOf",
        "anyOf",
        "type",
        "enum",
        "const",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
    ]
    .iter()
    .any(|key| is(name, key))
}
fn stripped(name: &[u16]) -> bool {
    name.starts_with(&[b'x' as u16, b'-' as u16])
        || [
            "title",
            "description",
            "examples",
            "default",
            "deprecated",
            "readOnly",
            "writeOnly",
            "$schema",
            "format",
            "discriminator",
            "nullable",
            "$defs",
        ]
        .iter()
        .any(|key| is(name, key))
}
pub(crate) fn outside(
    object: &Object,
    depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    attempt.depth(depth)?;
    attempt.work(object.len().saturating_mul(512).saturating_add(1))?;
    if object.keys().any(|key| !known(key)) {
        return Ok(true);
    }
    if let Some(Value::Array(branches)) = get(object, "allOf") {
        for branch in branches {
            if let Value::Object(object) = branch
                && outside(object, depth + 1, attempt)?
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(crate) fn normalize(input: &SchemaInput, attempt: &mut Attempt<'_>) -> Result<Value, Failure> {
    let resources = Resources::new(input, attempt)?;
    let active = attempt.vector(129)?;
    Engine { resources, active }.schema(input.schema.view(), 0, 0, attempt)
}
struct Engine {
    resources: Resources,
    active: Vec<((usize, usize), usize)>,
}
impl Engine {
    fn schema(
        &mut self,
        schema: JsonRef<'_>,
        scope: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<Value, Failure> {
        attempt.depth(depth)?;
        let identity = (backend::occurrence(schema), scope);
        attempt.work(self.active.len().saturating_add(1))?;
        if self.active.contains(&identity) {
            return Err(Failure::ReferenceCycle);
        }
        self.active.push(identity);
        let result = self.inner(schema, scope, depth, attempt);
        self.active.pop();
        result
    }
    fn inner(
        &mut self,
        schema: JsonRef<'_>,
        scope: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<Value, Failure> {
        if let Some(boolean) = schema.as_bool() {
            let mut object = Object::new();
            if !boolean {
                let key = value::literal("not", attempt)?;
                value::insert(&mut object, key, Value::Object(Object::new()), attempt)?;
            }
            return Ok(Value::Object(object));
        }
        let members = schema.members().ok_or(Failure::InvalidSchema)?;
        let mut raw = BTreeMap::new();
        // A schema has only fixed-size keyword probes below. Reserving their
        // aggregate work by member count also covers outside-keyword classification.
        attempt.work(members.len().saturating_mul(1024).saturating_add(1))?;
        for member in members {
            let key = value::name(member.name, attempt)?;
            value::insert(&mut raw, key, member.value.to_owned(), attempt)?;
        }
        if let Some(reference) = get(&raw, "$ref") {
            let (target, target_scope) =
                self.resources.resolve(scope, reference.view(), attempt)?;
            return self.schema(target.view(), target_scope, depth + 1, attempt);
        }
        let nullable = get(&raw, "nullable").and_then(|value| value.view().as_bool()) == Some(true);
        let mut object = Object::new();
        let mut branches = None;
        for (key, raw) in raw {
            if stripped(&key) {
                continue;
            }
            let raw = raw.view();
            let normalized = if is(&key, "type") {
                strings(raw, true, nullable, attempt)?
            } else if is(&key, "required") {
                strings(raw, false, false, attempt)?
            } else if is(&key, "properties") {
                let mut properties = Object::new();
                for member in raw.members().ok_or(Failure::InvalidSchema)? {
                    let key = value::name(member.name, attempt)?;
                    let child = self.schema(member.value, scope, depth + 1, attempt)?;
                    value::insert(&mut properties, key, child, attempt)?;
                }
                Value::Object(properties)
            } else if is(&key, "items")
                || (is(&key, "additionalProperties") && raw.kind() == JsonKind::Object)
            {
                self.schema(raw, scope, depth + 1, attempt)?
            } else if is(&key, "additionalProperties") {
                Value::Boolean(raw.as_bool().ok_or(Failure::InvalidSchema)?)
            } else if is(&key, "allOf") || is(&key, "oneOf") || is(&key, "anyOf") {
                let elements = raw.elements().ok_or(Failure::InvalidSchema)?;
                if elements.len() == 0 {
                    return Err(Failure::InvalidSchema);
                }
                let mut values = attempt.vector(elements.len())?;
                for child in elements {
                    values.push(self.schema(child, scope, depth + 1, attempt)?);
                }
                if is(&key, "allOf") {
                    branches = Some(values);
                    continue;
                }
                Value::Union(values)
            } else {
                if is(&key, "enum") && raw.kind() != JsonKind::Array {
                    return Err(Failure::InvalidSchema);
                }
                if [
                    "minimum",
                    "maximum",
                    "exclusiveMinimum",
                    "exclusiveMaximum",
                    "minLength",
                    "maxLength",
                    "minItems",
                    "maxItems",
                ]
                .iter()
                .any(|name| is(&key, name))
                    && raw.kind() != JsonKind::Number
                {
                    return Err(Failure::InvalidSchema);
                }
                value::import(raw, depth + 1, attempt)?
            };
            value::insert(&mut object, key, normalized, attempt)?;
        }
        match branches {
            Some(branches) => merge::all_of(object, branches, depth, attempt),
            None => Ok(Value::Object(object)),
        }
    }
}
fn strings(
    raw: JsonRef<'_>,
    types: bool,
    nullable: bool,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Failure> {
    let mut names = BTreeMap::new();
    let mut add = |raw: JsonRef<'_>| -> Result<(), Failure> {
        let key = value::name(raw, attempt)?;
        if types
            && ![
                "null", "boolean", "object", "array", "number", "integer", "string",
            ]
            .iter()
            .any(|name| is(&key, name))
        {
            return Err(Failure::InvalidSchema);
        }
        // Deduplication is specifically part of type/required normalization.
        attempt.work(
            key.len()
                .saturating_add(1)
                .saturating_mul(12)
                .saturating_mul(names.len().saturating_add(1).ilog2() as usize + 1),
        )?;
        if !names.contains_key(&key) {
            value::insert(&mut names, key, (), attempt)?;
        }
        Ok(())
    };
    if types && raw.kind() == JsonKind::String {
        add(raw)?;
    } else {
        let elements = raw.elements().ok_or(Failure::InvalidSchema)?;
        if types && elements.len() == 0 {
            return Err(Failure::InvalidSchema);
        }
        for item in elements {
            add(item)?;
        }
    }
    if nullable {
        let null = value::literal("null", attempt)?;
        if !names.contains_key(&null) {
            value::insert(&mut names, null, (), attempt)?;
        }
    }
    let mut result = attempt.vector(names.len())?;
    for (name, ()) in names {
        result.push(Value::String(name));
    }
    Ok(Value::Array(result))
}
