//! Optional JSON Schema 2020-12 evaluation. Resources are explicit; no I/O.
#![forbid(unsafe_code)]
mod literals;
use openbindings::*;
use openbindings_internal_json::{
    backend::{self, FlatJson},
    numeric,
};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub evaluation_steps: usize,
    pub evaluation_depth: usize,
    pub regex_steps: usize,
    pub max_problems: usize,
    pub compile_json_depth: usize,
    pub pattern_bytes: usize,
    pub pattern_depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            evaluation_steps: 2_000_000,
            evaluation_depth: 1024,
            regex_steps: 2_000_000,
            max_problems: 256,
            compile_json_depth: 512,
            pattern_bytes: 1024 * 1024,
            pattern_depth: 256,
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct DefaultEvaluator {
    limits: Limits,
}
impl DefaultEvaluator {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_limits(limits: Limits) -> Self {
        Self { limits }
    }
    pub fn limits(&self) -> &Limits {
        &self.limits
    }
}
#[derive(Clone)]
struct NoRetrieval;
impl jsonschema::Retrieve for NoRetrieval {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err(format!("resource is not in the prepared program: {uri}").into())
    }
}
fn no_verdict(reason: NoVerdictReason, code: &str, message: impl Into<String>) -> NoVerdict {
    NoVerdict::new(reason, code, message)
}
impl SchemaEvaluator for DefaultEvaluator {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        control.check()?;
        let mut program = request.evaluation_program(control)?;
        let mut registry = jsonschema::Registry::new().retriever(NoRetrieval);
        let mut literals = literals::Literals::default();
        for resource in &program.resources {
            control.check()?;
            check_patterns(resource.document.view(), &self.limits, control)?;
            let value = literals.resource(&resource.document, &self.limits, control)?;
            // Transfer each private projection into the registry. Holding a
            // second serde tree until compilation unnecessarily raises peak RSS.
            registry = registry.add(&resource.uri, value).map_err(|e| {
                no_verdict(
                    NoVerdictReason::EvaluatorFailure,
                    "program-resource",
                    e.to_string(),
                )
            })?;
        }
        let registry = registry.prepare().map_err(|e| {
            no_verdict(
                NoVerdictReason::ConservativePreparation,
                "program-registry",
                e.to_string(),
            )
        })?;
        let entry_uri = jsonschema::Uri::parse(program.entry_uri.clone()).map_err(|e| {
            no_verdict(
                NoVerdictReason::EvaluatorFailure,
                "program-entry-uri",
                e.0.to_string(),
            )
        })?;
        let root = registry.resolver(entry_uri).lookup("").map_err(|e| {
            no_verdict(
                NoVerdictReason::EvaluatorFailure,
                "missing-program-entry",
                e.to_string(),
            )
        })?;
        let validator = literals
            .apply(numeric::apply(jsonschema::options_for::<FlatJson>()))
            .with_registry(&registry)
            .with_draft(jsonschema::Draft::Draft202012)
            .with_retriever(NoRetrieval)
            .should_validate_formats(false)
            .with_pattern_options(
                jsonschema::PatternOptions::fancy_regex().backtrack_limit(self.limits.regex_steps),
            )
            .build(root.contents())
            .map_err(|e| {
                no_verdict(
                    NoVerdictReason::ConservativePreparation,
                    "evaluator-preparation",
                    e.to_string(),
                )
            })?;
        control.check()?;
        // Compilation owns the state it needs. Keep the original-location map,
        // but release the intermediate source-backed projection arenas.
        program.resources.clear();
        Ok(Arc::new(Compiled {
            validator,
            program,
            limits: self.limits.clone(),
        }))
    }
}
struct Compiled {
    validator: jsonschema::Validator<FlatJson>,
    program: EvaluationProgram,
    limits: Limits,
}
impl PreparedSchema for Compiled {
    fn validate(&self, value: &JsonValue, control: &WorkControl) -> ValueOutcome {
        if let Err(detail) = control.check() {
            return ValueOutcome::NoVerdict { detail };
        }
        if backend::has_unpaired(value) {
            return ValueOutcome::NoVerdict {
                detail: no_verdict(
                    NoVerdictReason::UnsupportedCapability,
                    "lone-surrogate-instance",
                    "instance interpretation requires Unicode scalar strings",
                ),
            };
        }
        let result = jsonschema::ob_work::bounded(
            self.limits.evaluation_steps,
            self.limits.evaluation_depth,
            || {
                jsonschema::ob_ecma::top_level(self.limits.regex_steps, || {
                    self.validator.is_valid(backend::view(value))
                })
            },
        );
        let valid = match result {
            Ok(Ok(valid)) => valid,
            Ok(Err(jsonschema::ob_ecma::Exhausted::UnsupportedProperty)) => {
                return ValueOutcome::NoVerdict {
                    detail: no_verdict(
                        NoVerdictReason::UnsupportedCapability,
                        "unicode-property-matching",
                        "Unicode property-escape matching is outside the qualified evaluator capability",
                    ),
                };
            }
            Ok(Err(_)) => {
                return ValueOutcome::NoVerdict {
                    detail: no_verdict(
                        NoVerdictReason::LimitExceeded,
                        "regex-work-limit",
                        "regular-expression work limit reached",
                    ),
                };
            }
            Err(reason) => {
                return ValueOutcome::NoVerdict {
                    detail: no_verdict(
                        if reason == jsonschema::ob_work::Stop::Cycle {
                            NoVerdictReason::ConservativePreparation
                        } else {
                            NoVerdictReason::LimitExceeded
                        },
                        if reason == jsonschema::ob_work::Stop::Arithmetic {
                            "numeric-arithmetic-limit"
                        } else {
                            "evaluation-work-limit"
                        },
                        format!("evaluation stopped: {reason:?}"),
                    ),
                };
            }
        };
        if let Err(detail) = control.check() {
            return ValueOutcome::NoVerdict { detail };
        }
        if valid {
            return ValueOutcome::Satisfies;
        }
        let mut problems = Vec::new();
        let mut complete = true;
        let diagnostics = jsonschema::ob_work::diagnostics(
            self.limits.max_problems.max(1).saturating_mul(8),
            || {
                jsonschema::ob_work::bounded(
                    self.limits.evaluation_steps,
                    self.limits.evaluation_depth,
                    || {
                        jsonschema::ob_ecma::top_level(self.limits.regex_steps, || {
                            for error in self.validator.iter_errors(backend::view(value)) {
                                if problems.len() >= self.limits.max_problems.max(1) {
                                    complete = false;
                                    break;
                                }
                                let schema_location = error
                                    .absolute_keyword_location()
                                    .and_then(|uri| self.program.original_location(uri.as_str()));
                                let paths = diagnostic_paths(&error, value);
                                for path in paths {
                                    if problems.len() >= self.limits.max_problems.max(1) {
                                        complete = false;
                                        break;
                                    }
                                    problems.push(ValueProblem {
                                        instance_pointer: path,
                                        schema_location: schema_location.clone(),
                                        code: error.kind().keyword().into(),
                                        message: diagnostic_message(error.kind()).into(),
                                    });
                                }
                                if control.is_cancelled() {
                                    complete = false;
                                    break;
                                }
                            }
                        })
                    },
                )
            },
        );
        if !matches!(diagnostics, Ok(Ok(Ok(())))) {
            complete = false;
        }
        if let Err(detail) = control.check() {
            return ValueOutcome::NoVerdict { detail };
        }
        ValueOutcome::Mismatch {
            problems,
            problems_complete: complete,
        }
    }
}
fn check_patterns(
    value: JsonRef<'_>,
    limits: &Limits,
    control: &WorkControl,
) -> Result<(), NoVerdict> {
    // Only schema pattern positions count; opaque values are not interpreted.
    let Some(definitions) = value.get("$defs").and_then(|v| v.members()) else {
        return Ok(());
    };
    for definition in definitions {
        control.check()?;
        let schema = definition.value;
        let mut patterns = Vec::new();
        if let Some(pattern) = schema.get("pattern").and_then(|v| v.as_str()) {
            patterns.push(pattern);
        }
        if let Some(entries) = schema.get("patternProperties").and_then(|v| v.members()) {
            patterns.extend(entries.filter_map(|m| m.name.as_str()));
        }
        for pattern in patterns {
            if pattern.len() > limits.pattern_bytes {
                return Err(no_verdict(
                    NoVerdictReason::LimitExceeded,
                    "pattern-byte-limit",
                    "pattern exceeds the admitted byte limit",
                ));
            }
            let mut escaped = false;
            let mut class = false;
            let mut depth = 0usize;
            for byte in pattern.bytes() {
                if escaped {
                    escaped = false;
                    continue;
                }
                match byte {
                    b'\\' => escaped = true,
                    b'[' if !class => class = true,
                    b']' if class => class = false,
                    b'(' if !class => {
                        depth += 1;
                        if depth > limits.pattern_depth {
                            return Err(no_verdict(
                                NoVerdictReason::LimitExceeded,
                                "pattern-depth-limit",
                                "pattern nesting exceeds the admitted limit",
                            ));
                        }
                    }
                    b')' if !class => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn diagnostic_paths(error: &jsonschema::ValidationError<'_>, value: &JsonValue) -> Vec<String> {
    use jsonschema::error::ValidationErrorKind as Kind;
    let root = error.instance_path().as_str();
    let child = |name: &str| format!("{root}/{}", name.replace('~', "~0").replace('/', "~1"));
    match error.kind() {
        Kind::AdditionalProperties { unexpected } | Kind::UnevaluatedProperties { unexpected } => {
            unexpected.iter().map(|name| child(name)).collect()
        }
        Kind::AdditionalItems { limit } => value
            .at(root)
            .and_then(|v| v.elements())
            .map(|a| (*limit..a.len()).map(|i| child(&i.to_string())).collect())
            .unwrap_or_else(|| vec![root.into()]),
        Kind::UnevaluatedItems { indexes, .. } if !indexes.is_empty() => {
            indexes.iter().map(|i| child(&i.to_string())).collect()
        }
        _ => vec![root.into()],
    }
}
fn diagnostic_message(kind: &jsonschema::error::ValidationErrorKind) -> &'static str {
    use jsonschema::error::ValidationErrorKind as K;
    match kind {
        K::Type { .. } => "value has a type not allowed by this schema",
        K::Required { .. } => {
            "object is missing a required member; inspect the required keyword at the schema location"
        }
        K::AdditionalProperties { .. } | K::UnevaluatedProperties { .. } => {
            "this member is not permitted by the schema"
        }
        K::AdditionalItems { .. } | K::UnevaluatedItems { .. } => {
            "this array item is not permitted by the schema"
        }
        K::Pattern { .. } => "string does not match the schema pattern",
        K::PropertyNames { .. } => {
            "object contains a member name that does not satisfy the propertyNames schema"
        }
        K::Constant { .. } | K::Enum { .. } => {
            "value is not one of the values allowed by this schema"
        }
        K::UniqueItems => "array contains equal items where unique items are required",
        _ => "value does not satisfy the constraint at the schema location",
    }
}
