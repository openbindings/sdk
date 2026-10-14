//! Keep literal values in the exact flat representation. Only the shallow
//! projected schema structure crosses the dependency's recursive serde boundary.
use crate::{Limits, no_verdict};
use jsonschema::{Keyword, ValidationError, ValidationOptions};
use openbindings::*;
use openbindings_internal_json::backend::{self, FlatJson, View};
use serde_json::{Map, Value};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct Operands {
    literals: Vec<Arc<[JsonValue]>>,
    numbers: openbindings_internal_json::numeric::Operands,
}
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
impl Operands {
    pub(crate) fn resource(
        &mut self,
        resource: &JsonValue,
        limits: &Limits,
        control: &dyn WorkControl,
    ) -> Result<Value, NoVerdict> {
        // Partial bounds include a private constant resource with no source map.
        if let Some(value) = resource.view().as_bool() {
            return Ok(Value::Bool(value));
        }
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
                                    "minimum" | "maximum" | "exclusiveMinimum"
                                    | "exclusiveMaximum" | "multipleOf" => {
                                        self.numbers.insert(field.value.text())
                                    }
                                    // No finite JSON collection can reach u64::MAX elements.
                                    // Preserve contains annotations by keeping its native keyword;
                                    // normalize every size operand without exponent expansion.
                                    "minLength" | "maxLength" | "minItems" | "maxItems"
                                    | "minProperties" | "maxProperties" | "minContains"
                                    | "maxContains" => Value::from(
                                        openbindings_internal_json::decimal::Decimal::parse(
                                            field.value.text(),
                                        )
                                        .nonnegative_integer_u64_saturating()
                                        .expect("admitted size operand"),
                                    ),
                                    "const" => {
                                        let id = self.literals.len();
                                        self.literals.push(Arc::from([field.value.to_owned()]));
                                        Value::from(id)
                                    }
                                    "enum" => {
                                        let choices: Vec<_> = field
                                            .value
                                            .elements()
                                            .expect("admitted enum array")
                                            .map(|v| v.to_owned())
                                            .collect();
                                        let id = self.literals.len();
                                        self.literals.push(choices.into());
                                        Value::Array(vec![Value::from(id)])
                                    }
                                    // Annotations and unknown-keyword data do not affect this
                                    // validity-only API. Their original bytes stay in the SDK.
                                    "$ref"
                                    | "$dynamicRef"
                                    | "$dynamicAnchor"
                                    | "type"
                                    | "pattern"
                                    | "uniqueItems"
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
        options = self.numbers.apply(options);
        let literals = Arc::new(self.literals);
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
    serde::Deserialize::deserialize(&mut decoder).map_err(|_| {
        no_verdict(
            NoVerdictReason::EvaluatorFailure,
            "program-decoding",
            "the projected schema could not be decoded",
        )
    })
}
