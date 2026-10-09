//! Keep literal values in the exact flat representation. Only the shallow
//! projected schema structure crosses the dependency's recursive serde boundary.
use crate::{Limits, no_verdict};
use jsonschema::{Keyword, ValidationError, ValidationOptions};
use openbindings::*;
use openbindings_internal_json::backend::{self, FlatJson, View};
use serde_json::{Map, Value};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct Literals(Vec<Arc<[JsonValue]>>);
struct Constraint(Arc<[JsonValue]>);
impl<'i> Keyword<'i, FlatJson> for Constraint {
    fn is_valid(&self, instance: View<'i>) -> bool {
        self.0
            .iter()
            .any(|literal| backend::equals(instance, backend::view(literal)))
    }
    fn validate(&self, instance: View<'i>) -> Result<(), ValidationError<'i>> {
        if self.is_valid(instance) {
            Ok(())
        } else {
            Err(ValidationError::custom(
                "value differs from the required exact literal",
            ))
        }
    }
}
impl Literals {
    pub(crate) fn resource(
        &mut self,
        resource: &JsonValue,
        limits: &Limits,
        control: &WorkControl,
    ) -> Result<Value, NoVerdict> {
        let mut root = Map::new();
        for member in resource
            .view()
            .members()
            .expect("projected resource is an object")
        {
            control.check()?;
            let key = member.name.as_str().expect("projected member name");
            if key == "$defs" {
                let mut defs = Map::new();
                for member in member.value.members().expect("projected definitions map") {
                    control.check()?;
                    let value = match member.value.kind() {
                        JsonKind::Boolean => Value::Bool(member.value.as_bool().unwrap()),
                        _ => {
                            let mut schema = Map::new();
                            for field in member.value.members().expect("admitted schema object") {
                                let key = field.name.as_str().expect("admitted scalar name");
                                let value = match key {
                                    "const" => {
                                        let id = self.0.len();
                                        self.0.push(Arc::from([field.value.to_owned()]));
                                        Value::from(id)
                                    }
                                    "enum" => {
                                        let choices: Vec<_> = field
                                            .value
                                            .elements()
                                            .expect("admitted enum array")
                                            .map(|v| v.to_owned())
                                            .collect();
                                        let id = self.0.len();
                                        self.0.push(choices.into());
                                        Value::Array(vec![Value::from(id)])
                                    }
                                    // Annotations and unknown-keyword data do not affect this
                                    // validity-only API. Their original bytes stay in the SDK.
                                    "$ref"
                                    | "$dynamicRef"
                                    | "$dynamicAnchor"
                                    | "type"
                                    | "multipleOf"
                                    | "maximum"
                                    | "exclusiveMaximum"
                                    | "minimum"
                                    | "exclusiveMinimum"
                                    | "maxLength"
                                    | "minLength"
                                    | "pattern"
                                    | "maxItems"
                                    | "minItems"
                                    | "uniqueItems"
                                    | "maxContains"
                                    | "minContains"
                                    | "maxProperties"
                                    | "minProperties"
                                    | "required"
                                    | "dependentRequired"
                                    | "properties"
                                    | "patternProperties"
                                    | "additionalProperties"
                                    | "propertyNames"
                                    | "dependentSchemas"
                                    | "unevaluatedProperties"
                                    | "items"
                                    | "prefixItems"
                                    | "contains"
                                    | "unevaluatedItems"
                                    | "not"
                                    | "if"
                                    | "then"
                                    | "else"
                                    | "allOf"
                                    | "anyOf"
                                    | "oneOf" => read(field.value, limits)?,
                                    _ => continue,
                                };
                                schema.insert(key.into(), value);
                            }
                            Value::Object(schema)
                        }
                    };
                    defs.insert(member.name.as_str().unwrap().into(), value);
                }
                root.insert(key.into(), Value::Object(defs));
            } else {
                root.insert(key.into(), read(member.value, limits)?);
            }
        }
        Ok(Value::Object(root))
    }
    pub(crate) fn apply<R>(
        self,
        mut options: ValidationOptions<'_, R, FlatJson>,
    ) -> ValidationOptions<'_, R, FlatJson> {
        let literals = Arc::new(self.0);
        for keyword in ["const", "enum"] {
            let literals = literals.clone();
            options = options.with_keyword(keyword, move |_, value, _| {
                let id = if keyword == "enum" {
                    value
                        .as_array()
                        .and_then(|v| v.first())
                        .and_then(Value::as_u64)
                } else {
                    value.as_u64()
                };
                let literal = id
                    .and_then(|id| literals.get(id as usize))
                    .ok_or_else(|| ValidationError::custom("invalid internal literal reference"))?;
                Ok(Box::new(Constraint(literal.clone())))
            });
        }
        options
    }
}
fn read(value: JsonRef<'_>, limits: &Limits) -> Result<Value, NoVerdict> {
    if backend::depth(&value.to_owned()) > limits.compile_json_depth {
        return Err(no_verdict(
            NoVerdictReason::LimitExceeded,
            "compile-json-depth",
            "projected schema structure exceeds the dependency compilation-depth admission limit",
        ));
    }
    let mut decoder = serde_json::Deserializer::from_slice(value.text().as_bytes());
    decoder.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut decoder).map_err(|e| {
        no_verdict(
            NoVerdictReason::EvaluatorFailure,
            "program-decoding",
            e.to_string(),
        )
    })
}
