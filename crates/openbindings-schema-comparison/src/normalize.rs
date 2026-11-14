use crate::ComparisonFailureKind as FailureKind;
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
pub(crate) fn get_value<'a>(map: &'a Object, text: &str) -> Option<&'a Value> {
    get(map, text).map(Value::view)
}
pub(crate) fn known(name: &[u16]) -> bool {
    [
        "allOf",
        "$defs",
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
pub(crate) fn stripped(name: &[u16]) -> bool {
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
            if let Value::Object(object) = branch.view()
                && outside(object, depth + 1, attempt)?
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

// Retention belongs to this schema's own schema subtree, never its resource.
fn keep_defs(
    schema: JsonRef<'_>,
    _depth: usize,
    attempt: &mut Attempt<'_>,
) -> Result<bool, Failure> {
    attempt.scratch(|attempt| keep_defs_scratch(schema, attempt))
}
fn keep_defs_scratch(schema: JsonRef<'_>, attempt: &mut Attempt<'_>) -> Result<bool, Failure> {
    let mut pending = attempt.vector(1)?;
    pending.push(schema.to_owned());
    while let Some(schema) = pending.pop() {
        if let Some(members) = schema.view().members() {
            for member in members {
                let key = value::name(member.name, attempt)?;
                if !known(&key) && !stripped(&key) && !is(&key, "$ref") && !is(&key, "$defs") {
                    return Ok(true);
                }
            }
        }
        let mut failure = None;
        openbindings_internal_schema::schema_index::each_schema_child(schema.view(), |child| {
            if failure.is_none() {
                match attempt.allocate(2 * std::mem::size_of::<openbindings::JsonValue>()) {
                    Ok(()) => pending.push(child.to_owned()),
                    Err(e) => failure = Some(e),
                }
            }
        });
        if let Some(error) = failure {
            return Err(error);
        }
    }
    Ok(false)
}
fn retained_applicator(key: &[u16]) -> bool {
    [
        "not",
        "if",
        "then",
        "else",
        "dependentSchemas",
        "prefixItems",
        "contains",
        "patternProperties",
        "propertyNames",
        "unevaluatedItems",
        "unevaluatedProperties",
    ]
    .iter()
    .any(|s| is(key, s))
}
pub(crate) fn normalize(
    input: &SchemaInput,
    side: crate::ComparisonSide,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Failure> {
    let resources = Resources::new(input, side, attempt)?;
    let active = attempt.vector(129)?;
    Engine::<false> {
        resources,
        active,
        leaves: None,
        accounted_for: None,
        path: Vec::new(),
    }
    .schema(input.schema.view(), 0, 0, attempt)
}
pub(crate) fn normalize_with_references(
    input: &SchemaInput,
    side: crate::ComparisonSide,
    leaves: &BTreeMap<crate::reference_identity::ReferenceKey, ()>,
    accounted_for: &dyn Fn(crate::ReferenceContext<'_>) -> bool,
    attempt: &mut Attempt<'_>,
) -> Result<Value, Failure> {
    let resources = Resources::new(input, side, attempt)?;
    let active = attempt.vector(129)?;
    Engine::<true> {
        resources,
        active,
        leaves: Some(leaves),
        accounted_for: Some(accounted_for),
        path: Vec::new(),
    }
    .schema(input.schema.view(), 0, 0, attempt)
}
struct Engine<'a, const REFERENCES: bool> {
    path: Name,
    leaves: Option<&'a BTreeMap<crate::reference_identity::ReferenceKey, ()>>,
    accounted_for: Option<&'a dyn Fn(crate::ReferenceContext<'_>) -> bool>,
    resources: Resources,
    active: Vec<((usize, usize), usize)>,
}
impl<const REFERENCES: bool> Engine<'_, REFERENCES> {
    fn child(
        &mut self,
        schema: JsonRef<'_>,
        scope: usize,
        depth: usize,
        position: (&[u16], Option<&Name>),
        retained: bool,
        attempt: &mut Attempt<'_>,
    ) -> Result<Value, Failure> {
        if !REFERENCES {
            return if retained {
                self.retained_schema(schema, scope, depth, attempt)
            } else {
                self.schema(schema, scope, depth, attempt)
            };
        }
        let mut path = crate::reference_identity::segment(&self.path, position.0, attempt)?;
        if let Some(name) = position.1 {
            path = crate::reference_identity::segment(&path, name, attempt)?;
        }
        let previous = std::mem::replace(&mut self.path, path);
        let result = if retained {
            self.retained_schema(schema, scope, depth, attempt)
        } else {
            self.schema(schema, scope, depth, attempt)
        };
        self.path = previous;
        result
    }
    fn schema(
        &mut self,
        schema: JsonRef<'_>,
        scope: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<Value, Failure> {
        attempt.depth(depth)?;
        let scope = self.resources.enter(schema, scope, attempt)?;
        let identity = (backend::occurrence(schema), scope);
        attempt.work(self.active.len().saturating_add(1))?;
        if self.active.contains(&identity) {
            return Err(self.resources.locate_failure(
                Failure::from(FailureKind::ReferenceCycle),
                schema,
                scope,
                attempt,
            ));
        }
        self.active.push(identity);
        let result = self.inner(schema, scope, depth, attempt);
        self.active.pop();
        result.map_err(|failure| {
            self.resources
                .locate_failure(failure, schema, scope, attempt)
        })
    }
    fn target_schema(
        &mut self,
        schema: JsonRef<'_>,
        scope: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<Value, Failure> {
        let scope = self.resources.enter(schema, scope, attempt)?;
        let key = (
            self.resources.identity(),
            backend::occurrence(schema),
            scope,
            depth,
        );
        if !REFERENCES {
            attempt.work(attempt.normalized.len().saturating_add(1).ilog2() as usize + 1)?;
            if let Some(value) = attempt.normalized.get(&key).cloned() {
                return Ok(Value::Shared(value));
            }
        }
        let mark = attempt.allocation_mark();
        let result = self.schema(schema, scope, depth, attempt)?;
        if !REFERENCES {
            attempt.allocate(2048)?;
            let cached = match result {
                Value::Shared(value) => value,
                value => std::sync::Arc::new(value),
            };
            attempt.normalized.insert(key, cached.clone());
            // The target and its conservative construction estimate remain
            // owned by the shared cache for the rest of this comparison.
            attempt.retain_since(mark);
            return Ok(Value::Shared(cached));
        }
        Ok(result)
    }
    fn keep_reference(
        &self,
        schema: JsonRef<'_>,
        scope: usize,
        dynamic: bool,
        attempt: &mut Attempt<'_>,
    ) -> Result<bool, Failure> {
        if !REFERENCES {
            return Ok(false);
        }
        let leaves = self.leaves.expect("reference variant");
        attempt.work(leaves.len().saturating_add(1).ilog2() as usize + 1)?;
        attempt.allocate(self.path.len().saturating_mul(2))?;
        if !leaves.contains_key(&(backend::occurrence(schema), dynamic, self.path.clone())) {
            return Ok(false);
        }
        let reference = schema
            .get(if dynamic { "$dynamicRef" } else { "$ref" })
            .expect("reference leaf");
        let spelling = reference.as_str().ok_or(FailureKind::InvalidReference)?;
        Ok((self.accounted_for.expect("reference variant"))(
            self.resources.context(scope, schema, spelling, attempt)?,
        ))
    }
    fn retained_reference(
        &mut self,
        reference: JsonRef<'_>,
        scope: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<Value, Failure> {
        let (target, target_scope) = self.resources.resolve(scope, reference, attempt)?;
        let authored = value::import(reference, depth, attempt)?;
        let target = self.target_schema(target.view(), target_scope, depth + 1, attempt)?;
        attempt.allocate(2 * std::mem::size_of::<Value>())?;
        Ok(Value::Reference(Box::new(authored), Box::new(target)))
    }
    fn retained_keyword(
        &mut self,
        key: &[u16],
        raw: JsonRef<'_>,
        scope: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<Value, Failure> {
        if is(key, "dependentSchemas") || is(key, "patternProperties") || is(key, "properties") {
            let mut object = Object::new();
            for member in raw.members().ok_or(FailureKind::InvalidSchema)? {
                let name = value::name(member.name, attempt)?;
                let child = self.child(
                    member.value,
                    scope,
                    depth + 1,
                    (key, Some(&name)),
                    true,
                    attempt,
                )?;
                value::insert(&mut object, name, child, attempt)?;
            }
            Ok(Value::Object(object))
        } else if ["prefixItems", "allOf", "oneOf", "anyOf"]
            .iter()
            .any(|s| is(key, s))
        {
            let elements = raw.elements().ok_or(FailureKind::InvalidSchema)?;
            let mut children = attempt.vector(elements.len())?;
            for child in elements {
                children.push(self.child(child, scope, depth + 1, (key, None), true, attempt)?);
            }
            Ok(Value::Array(children))
        } else {
            self.child(raw, scope, depth, (key, None), true, attempt)
        }
    }
    fn retained_schema(
        &mut self,
        schema: JsonRef<'_>,
        scope: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<Value, Failure> {
        attempt.depth(depth)?;
        let scope = self.resources.enter(schema, scope, attempt)?;
        let identity = (backend::occurrence(schema), scope);
        attempt.work(self.active.len().saturating_add(1))?;
        if self.active.contains(&identity) {
            return Err(self.resources.locate_failure(
                Failure::from(FailureKind::ReferenceCycle),
                schema,
                scope,
                attempt,
            ));
        }
        self.active.push(identity);
        let result = self.retained_inner(schema, scope, depth, attempt);
        self.active.pop();
        result.map_err(|failure| {
            self.resources
                .locate_failure(failure, schema, scope, attempt)
        })
    }
    fn retained_inner(
        &mut self,
        schema: JsonRef<'_>,
        scope: usize,
        depth: usize,
        attempt: &mut Attempt<'_>,
    ) -> Result<Value, Failure> {
        if schema.as_bool().is_some() {
            return value::import(schema, depth, attempt);
        }
        let mut object = Object::new();
        for member in schema.members().ok_or(FailureKind::InvalidSchema)? {
            let key = value::name(member.name, attempt)?;
            let child = if is(&key, "$ref") || is(&key, "$dynamicRef") {
                if self.keep_reference(schema, scope, is(&key, "$dynamicRef"), attempt)? {
                    value::import(member.value, depth + 1, attempt)?
                } else {
                    self.retained_reference(member.value, scope, depth + 1, attempt)?
                }
            } else if retained_applicator(&key)
                || [
                    "properties",
                    "items",
                    "additionalProperties",
                    "allOf",
                    "oneOf",
                    "anyOf",
                ]
                .iter()
                .any(|s| is(&key, s))
            {
                self.retained_keyword(&key, member.value, scope, depth + 1, attempt)?
            } else {
                value::import(member.value, depth + 1, attempt)?
            };
            value::insert(&mut object, key, child, attempt)?;
        }
        Ok(Value::Object(object))
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
        let members = schema.members().ok_or(FailureKind::InvalidSchema)?;
        let mut raw = BTreeMap::new();
        // A schema has only fixed-size keyword probes below. Reserving their
        // aggregate work by member count also covers outside-keyword classification.
        attempt.work(members.len().saturating_mul(1024).saturating_add(1))?;
        for member in members {
            let key = value::name(member.name, attempt)?;
            value::insert(&mut raw, key, member.value.to_owned(), attempt)?;
        }
        let keep =
            get(&raw, "$ref").is_some() && self.keep_reference(schema, scope, false, attempt)?;
        let target = if let Some(reference) = get(&raw, "$ref").filter(|_| !keep) {
            let (target, target_scope) =
                self.resources.resolve(scope, reference.view(), attempt)?;
            Some(self.target_schema(target.view(), target_scope, depth + 1, attempt)?)
        } else {
            None
        };
        let mut object = Object::new();
        let mut branches = None;
        for (key, raw) in raw {
            if stripped(&key)
                || (is(&key, "$ref") && !keep)
                || (is(&key, "$defs") && !keep_defs(schema, depth, attempt)?)
            {
                continue;
            }
            let raw = raw.view();
            let normalized = if is(&key, "type") {
                strings(raw, true, attempt)?
            } else if is(&key, "required") {
                strings(raw, false, attempt)?
            } else if is(&key, "properties") {
                let mut properties = Object::new();
                for member in raw.members().ok_or(FailureKind::InvalidSchema)? {
                    let name = value::name(member.name, attempt)?;
                    let child = self.child(
                        member.value,
                        scope,
                        depth + 1,
                        (&key, Some(&name)),
                        false,
                        attempt,
                    )?;
                    value::insert(&mut properties, name, child, attempt)?;
                }
                Value::Object(properties)
            } else if is(&key, "items")
                || (is(&key, "additionalProperties") && raw.kind() == JsonKind::Object)
            {
                self.child(raw, scope, depth + 1, (&key, None), false, attempt)?
            } else if is(&key, "additionalProperties") {
                Value::Boolean(raw.as_bool().ok_or(FailureKind::InvalidSchema)?)
            } else if is(&key, "allOf") || is(&key, "oneOf") || is(&key, "anyOf") {
                let elements = raw.elements().ok_or(FailureKind::InvalidSchema)?;
                if elements.len() == 0 {
                    return Err(Failure::from(FailureKind::InvalidSchema));
                }
                let mut values = attempt.vector(elements.len())?;
                for child in elements {
                    values.push(self.child(
                        child,
                        scope,
                        depth + 1,
                        (&key, None),
                        false,
                        attempt,
                    )?);
                }
                if is(&key, "allOf") {
                    branches = Some(values);
                    continue;
                }
                Value::Union(values)
            } else if retained_applicator(&key) {
                self.retained_keyword(&key, raw, scope, depth + 1, attempt)?
            } else if is(&key, "$dynamicRef") {
                if self.keep_reference(schema, scope, true, attempt)? {
                    value::import(raw, depth + 1, attempt)?
                } else {
                    self.retained_reference(raw, scope, depth + 1, attempt)?
                }
            } else {
                if is(&key, "enum") && raw.kind() != JsonKind::Array {
                    return Err(Failure::from(FailureKind::InvalidSchema));
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
                    return Err(Failure::from(FailureKind::InvalidSchema));
                }
                value::import(raw, depth + 1, attempt)?
            };
            value::insert(&mut object, key, normalized, attempt)?;
        }
        let siblings = match branches {
            Some(branches) => merge::all_of(object, branches, depth, attempt)?,
            None => Value::Object(object),
        };
        if let Some(target) = target {
            if matches!(&siblings, Value::Object(object) if object.is_empty()) {
                return Ok(target);
            }
            let mut conjunction = attempt.vector(2)?;
            conjunction.extend([target, siblings]);
            merge::all_of(Object::new(), conjunction, depth, attempt)
        } else {
            Ok(siblings)
        }
    }
}
fn strings(raw: JsonRef<'_>, types: bool, attempt: &mut Attempt<'_>) -> Result<Value, Failure> {
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
            return Err(Failure::from(FailureKind::InvalidSchema));
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
        let elements = raw.elements().ok_or(FailureKind::InvalidSchema)?;
        if types && elements.len() == 0 {
            return Err(Failure::from(FailureKind::InvalidSchema));
        }
        for item in elements {
            add(item)?;
        }
    }
    let mut result = attempt.vector(names.len())?;
    for (name, ()) in names {
        result.push(Value::String(name));
    }
    Ok(Value::Array(result))
}
