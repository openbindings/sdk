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
    // Each compiled `const` and `enum` keyword's literal, by the id it holds.
    literals: Vec<Literal>,
    numbers: openbindings_internal_json::numeric::Operands,
    dynamic_anchors: bool,
}
// A `const` value, or an `enum` keyword's choices. Both are views of the
// projection's separated literals.
#[derive(Clone)]
enum Literal {
    Value(JsonValue),
    Choices(Arc<[JsonValue]>),
}
struct Constraint(Literal);
impl<'i> Keyword<'i, FlatJson> for Constraint {
    fn is_valid(&self, instance: View<'i>) -> bool {
        match &self.0 {
            Literal::Value(value) => backend::equals(instance, backend::view(value)),
            Literal::Choices(choices) => choices
                .iter()
                .any(|choice| backend::equals(instance, backend::view(choice))),
        }
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
/// How the compiled validator receives one projected keyword.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// An exact numeric operand handled by the SDK's numeric keywords.
    Number,
    /// A size operand normalized for the library.
    Size,
    /// An exact literal handled by the SDK's literal keywords.
    Const,
    /// Exact literal choices handled by the SDK's literal keywords.
    Enum,
    /// A value the library reads as it is.
    Read,
    /// A `$dynamicAnchor`, also read as it is: references can then resolve
    /// through the dynamic scope.
    DynamicAnchor,
}
/// The role of a projected keyword, or `None` for keywords the validator never
/// compiles: annotations and unknown keywords. No library error names them.
pub(crate) fn role(keyword: &str) -> Option<Role> {
    Some(match keyword {
        "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" | "multipleOf" => {
            Role::Number
        }
        "minLength" | "maxLength" | "minItems" | "maxItems" | "minProperties" | "maxProperties"
        | "minContains" | "maxContains" => Role::Size,
        "const" => Role::Const,
        "enum" => Role::Enum,
        "$dynamicAnchor" => Role::DynamicAnchor,
        "$ref"
        | "$dynamicRef"
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
        | "oneOf" => Role::Read,
        _ => return None,
    })
}
impl Operands {
    /// The library's copy of a projected resource. Its `const` and `enum`
    /// values are indices into `literals`, the literals the projection
    /// separates, which the compiled keywords share rather than copy.
    pub(crate) fn resource(
        &mut self,
        resource: &JsonValue,
        literals: &[JsonValue],
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
                                let value = match role(key) {
                                    Some(Role::Number) => self.numbers.insert(field.value.text()),
                                    // No finite JSON collection can reach u64::MAX elements.
                                    // Preserve contains annotations by keeping its native keyword;
                                    // normalize every size operand without exponent expansion.
                                    Some(Role::Size) => Value::from(
                                        openbindings_internal_json::decimal::Decimal::parse(
                                            field.value.text(),
                                        )
                                        .nonnegative_integer_u64_saturating()
                                        .expect("admitted size operand"),
                                    ),
                                    Some(Role::Const) => {
                                        let value = separated(field.value, literals)?.clone();
                                        let id = self.literals.len();
                                        self.literals.push(Literal::Value(value));
                                        Value::from(id)
                                    }
                                    Some(Role::Enum) => {
                                        let choices: Vec<_> = separated(field.value, literals)?
                                            .view()
                                            .elements()
                                            .expect("admitted enum array")
                                            .map(|v| v.to_owned())
                                            .collect();
                                        let id = self.literals.len();
                                        self.literals.push(Literal::Choices(choices.into()));
                                        Value::Array(vec![Value::from(id)])
                                    }
                                    Some(Role::Read) => read(field.value, limits)?,
                                    Some(Role::DynamicAnchor) => {
                                        self.dynamic_anchors = true;
                                        read(field.value, limits)?
                                    }
                                    // Annotations and unknown-keyword data do not affect this
                                    // validity-only API. Their original bytes stay in the SDK.
                                    None => continue,
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
    /// Whether a converted schema declares `$dynamicAnchor`. Every projected
    /// schema is a definition of a 2020-12 resource, so without one no reference
    /// resolves through the dynamic scope.
    pub(crate) fn declares_dynamic_anchors(&self) -> bool {
        self.dynamic_anchors
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
/// The separated literal a projected `const` or `enum` names by its index.
pub(crate) fn separated<'a>(
    index: JsonRef<'_>,
    literals: &'a [JsonValue],
) -> Result<&'a JsonValue, NoVerdict> {
    index
        .number_text()
        .and_then(|index| index.parse::<usize>().ok())
        .and_then(|index| literals.get(index))
        .ok_or_else(|| {
            no_verdict(
                NoVerdictReason::EvaluatorFailure,
                "program-literal",
                "a projected literal is missing",
            )
        })
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
