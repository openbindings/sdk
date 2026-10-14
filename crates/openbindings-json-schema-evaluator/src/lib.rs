//! Optional JSON Schema 2020-12 evaluation. Resources are explicit; no I/O.
#![forbid(unsafe_code)]
#![warn(missing_docs)]
mod details;
mod operands;
#[cfg(test)]
extern crate self as openbindings_json_schema_evaluator;
mod planner;
use openbindings::*;
use openbindings_internal_json::backend::{self, FlatJson};
use openbindings_internal_schema::{
    Program as EvaluationProgram, ProjectedResource as SchemaResource,
};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
/// Finite default-evaluator budgets. Work counts are implementation units, not milliseconds; exhaustion yields no verdict unless an established failure only loses diagnostic completeness. Zero is literal. These budgets do not bound upstream diagnostic allocation or stack use.
pub struct Limits {
    /// Evaluation work units per verdict/diagnostic pass; default 2,000,000. A partial verdict shares this allowance across upper and lower evaluations.
    pub evaluation_steps: usize,
    /// Nested evaluation depth; default 1024, distinct from source JSON nesting.
    pub evaluation_depth: usize,
    /// Regular-expression work/backtracking budget per verdict/diagnostic pass; default 2,000,000. Upper and lower share one verdict allowance.
    pub regex_steps: usize,
    /// Maximum projected JSON nesting admitted to evaluator compilation; default 512.
    pub compile_json_depth: usize,
    /// Maximum UTF-8 bytes in each schema regular expression; default 1 MiB (1,048,576 bytes).
    pub pattern_bytes: usize,
    /// Maximum parenthesis nesting in a schema regular expression; default 256.
    pub pattern_depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            evaluation_steps: 2_000_000,
            evaluation_depth: 1024,
            regex_steps: 2_000_000,
            compile_json_depth: 512,
            pattern_bytes: 1024 * 1024,
            pattern_depth: 256,
        }
    }
}
/// Diagnostic policy for established value failures. Neither mode changes rule evidence.
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Diagnostics {
    /// Fixed value-free message, no problems, and incomplete diagnostics (default).
    #[default]
    Coarse,
    /// The library's first error, with redacted messages and original schema facts.
    /// Library-bounded: construction can allocate in proportion to inputs and can
    /// overflow the stack on deep values. The SDK promises no stronger bound.
    Upstream,
}
#[derive(Clone, Debug, Default)]
/// Optional JSON Schema 2020-12 companion with exact numeric semantics and explicit resources only. It performs no network/filesystem acquisition. Format is annotation, Unicode property-escape matching is not qualified, and potential non-progressing cycles can conservatively refuse. Use a custom [`SchemaEvaluator`] when different qualified capabilities are required.
pub struct DefaultEvaluator {
    limits: Limits,
    diagnostics: Diagnostics,
}
impl DefaultEvaluator {
    /// Construct the companion with [`Limits::default`]; no schemas are compiled or resources acquired yet.
    pub fn new() -> Self {
        Self::default()
    }
    /// Construct the companion with explicit budgets; limits apply during preparation/evaluation, not as wall-clock deadlines.
    pub fn with_limits(limits: Limits) -> Self {
        Self {
            limits,
            diagnostics: Diagnostics::Coarse,
        }
    }
    /// Select value diagnostics; defaults to [`Diagnostics::Coarse`].
    /// [`Diagnostics::Upstream`] requests the library's first error and retains
    /// original source snapshots for schema facts, which may contain secrets.
    /// Its allocations and stack use are library-bounded, not SDK-bounded.
    pub fn with_diagnostics(mut self, diagnostics: Diagnostics) -> Self {
        self.diagnostics = diagnostics;
        self
    }
    /// Borrow this evaluator's configured budgets without allocation.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }
}
#[derive(Clone)]
struct NoRetrieval;
impl jsonschema::Retrieve for NoRetrieval {
    fn retrieve(
        &self,
        _uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err("resource is not in the prepared program".into())
    }
}
fn no_verdict(reason: NoVerdictReason, code: &str, message: impl Into<String>) -> NoVerdict {
    NoVerdict::new(reason, code, message)
}
fn preparation_error(mut kind: &jsonschema::error::ValidationErrorKind) -> NoVerdict {
    use jsonschema::error::ValidationErrorKind;
    // Read only trusted kind metadata, never dependency Display text or schema
    // values. Property-name validation may wrap a regex-format failure.
    for _ in 0..16 {
        match kind {
            ValidationErrorKind::Format { format } if format == "regex" => {
                return no_verdict(
                    NoVerdictReason::ConservativePreparation,
                    "schema-pattern-compilation",
                    "a schema regular expression could not be compiled; inspect pattern and patternProperties",
                );
            }
            ValidationErrorKind::PropertyNames { error } => kind = error.kind(),
            _ => break,
        }
    }
    // Build errors can identify metaschema locations or resource-relative
    // projected paths without a resource identity. Neither proves an original
    // source location, so leave location absent rather than guessing the root.
    no_verdict(
        NoVerdictReason::ConservativePreparation,
        "evaluator-preparation",
        "the evaluator could not prepare the projected schema",
    )
}
impl SchemaEvaluator for DefaultEvaluator {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        control.check()?;
        let (space, entry) = planner::space(request);
        let cancellation = || control.is_cancelled();
        match space
            .program(entry, &cancellation)
            .map_err(projection_failure)
        {
            Ok(program) => Ok(Arc::new(self.compile(program, request, control)?)),
            Err(detail)
                if detail.reason == NoVerdictReason::ResourceUnavailable
                    && detail.code == "resource-unavailable" =>
            {
                let bounds = space
                    .bounds(entry, &cancellation)
                    .map_err(projection_failure)
                    .map_err(|decline| recover_partial_decline(detail, decline))?;
                let (lower, upper, unavailable) = bounds.into_parts();
                let unavailable = unavailable.map(projection_failure);
                let upper = self.compile(upper, request, control)?;
                control.check()?;
                let lower = self.compile(lower, request, control)?;
                control.check()?;
                Ok(Arc::new(PartialCompiled {
                    upper,
                    lower,
                    unavailable,
                }))
            }
            Err(detail) => Err(detail),
        }
    }
}
// Finite optional-planner declines only. Known-closure defects, shared admission
// limits, cancellation and compiler errors retain their actual classifications.
// With the planner's edge cap >= twice its node cap, schema-edge-limit already
// establishes too many known positions; supplying missing resources cannot cure
// it. The core cap-relationship regression pins this recovery-policy premise.
fn recover_partial_decline(strict: NoVerdict, planner: NoVerdict) -> NoVerdict {
    let optional_decline = matches!(
        (planner.reason, planner.code.as_str()),
        (
            NoVerdictReason::ConservativePreparation,
            "partial-nonpositive-influence" | "partial-annotation-or-dynamic"
        ) | (
            NoVerdictReason::LimitExceeded,
            "partial-program-byte-limit"
                | "partial-scratch-limit"
                | "schema-hole-limit"
                | "partial-generated-node-limit"
                | "partial-generated-edge-limit"
        )
    );
    if optional_decline { strict } else { planner }
}
impl DefaultEvaluator {
    fn compile(
        &self,
        mut program: EvaluationProgram,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Compiled, NoVerdict> {
        let mut registry = jsonschema::Registry::new().retriever(NoRetrieval);
        let mut literals = operands::Operands::default();
        for resource in &program.resources {
            control.check()?;
            check_patterns(resource, &program, &self.limits, control)?;
            let value = literals.resource(&resource.document, &self.limits, control)?;
            // Transfer each private projection into the registry. Holding a
            // second serde tree until compilation unnecessarily raises peak RSS.
            registry = registry.add(&resource.uri, value).map_err(|_| {
                no_verdict(
                    NoVerdictReason::EvaluatorFailure,
                    "program-resource",
                    "a projected resource could not be registered",
                )
            })?;
        }
        let registry = registry.prepare().map_err(|_| {
            no_verdict(
                NoVerdictReason::ConservativePreparation,
                "program-registry",
                "the projected schema registry could not be prepared",
            )
        })?;
        let entry_uri = jsonschema::Uri::parse(program.entry_uri.clone()).map_err(|_| {
            no_verdict(
                NoVerdictReason::EvaluatorFailure,
                "program-entry-uri",
                "the projected entry identifier is invalid",
            )
        })?;
        let root = registry.resolver(entry_uri).lookup("").map_err(|_| {
            no_verdict(
                NoVerdictReason::EvaluatorFailure,
                "missing-program-entry",
                "the projected schema entry could not be located",
            )
        })?;
        let validator = literals
            .apply(jsonschema::options_for::<FlatJson>())
            .with_registry(&registry)
            .with_draft(jsonschema::Draft::Draft202012)
            .with_retriever(NoRetrieval)
            .should_validate_formats(false)
            .with_pattern_options(
                jsonschema::PatternOptions::fancy_regex().backtrack_limit(self.limits.regex_steps),
            )
            .build(root.contents())
            .map_err(|error| preparation_error(error.kind()))?;
        control.check()?;
        // Clear direct resource owners while keeping the original-location map.
        // Compiled exact const/enum subviews can still retain projection arenas.
        program.resources.clear();
        Ok(Compiled {
            validator,
            program,
            limits: self.limits.clone(),
            detail_source: (self.diagnostics == Diagnostics::Upstream).then(|| request.clone()),
        })
    }
}
struct Compiled {
    detail_source: Option<SchemaRequest>,
    validator: jsonschema::Validator<FlatJson>,
    program: EvaluationProgram,
    limits: Limits,
}
impl PreparedSchema for Compiled {
    fn validate(&self, value: &JsonValue, control: &dyn WorkControl) -> ValueOutcome {
        if let Err(detail) = control.check() {
            let detail = detail.into();
            return ValueOutcome::NoVerdict { detail };
        }
        let valid = verdict_pass(&self.limits, || {
            Ok(self.validator.is_valid(backend::view(value)))
        });
        match valid.and_then(|valid| {
            control.check()?;
            Ok(valid)
        }) {
            Err(detail) => ValueOutcome::NoVerdict { detail },
            Ok(true) => ValueOutcome::Satisfies,
            Ok(false) => self.diagnostics(value, control, false),
        }
    }
}
impl Compiled {
    fn diagnostics(
        &self,
        value: &JsonValue,
        control: &dyn WorkControl,
        original_only: bool,
    ) -> ValueOutcome {
        let mut problems = Vec::new();
        if let Some(source) = &self.detail_source {
            // Validity is already established. Failure to obtain detail (including
            // cancellation or exhausted work) must never turn it into no-verdict.
            let diagnostic = verdict_pass(&self.limits, || {
                control.check()?;
                let Err(error) = self.validator.validate(backend::view(value)) else {
                    return Ok(None);
                };
                let location = error.absolute_keyword_location().and_then(|uri| {
                    self.program
                        .original_location(uri.as_str())
                        .map(original_location)
                });
                // Generated partial-program errors are not original schema evidence.
                if original_only && location.is_none() {
                    return Ok(None);
                }
                let detail =
                    match details::collect(source, location.as_ref(), error.kind(), control) {
                        details::Collection::Available(detail) => Some(detail),
                        _ => None,
                    };
                control.check()?;
                Ok(Some(ValueProblem {
                    instance_pointer: error.instance_path().as_str().into(),
                    schema_location: location,
                    code: error.kind().keyword().into(),
                    message: if original_only && error.kind().keyword() == "oneOf" {
                        "value does not satisfy exactly one alternative".into()
                    } else {
                        diagnostic_message(error.kind())
                    },
                    details: detail,
                }))
            });
            if let Ok(Some(problem)) = diagnostic {
                problems.push(problem);
            }
        }
        ValueOutcome::Fails {
            message: "value does not satisfy the contract".into(),
            problems,
            // A first error never claims exhaustive diagnostics.
            problems_complete: false,
        }
    }
}

struct PartialCompiled {
    upper: Compiled,
    lower: Compiled,
    unavailable: Option<NoVerdict>,
}
impl PreparedSchema for PartialCompiled {
    fn validate(&self, value: &JsonValue, control: &dyn WorkControl) -> ValueOutcome {
        if let Err(detail) = control.check() {
            let detail = detail.into();
            return ValueOutcome::NoVerdict { detail };
        }
        // One outer work AND regex allowance covers both Boolean evaluations.
        // Diagnostics start after this scope and never evaluate synthetic lower failures.
        let result = verdict_pass(&self.upper.limits, || {
            bounded_decision(
                || self.upper.validator.is_valid(backend::view(value)),
                || self.lower.validator.is_valid(backend::view(value)),
                control,
            )
        });
        match result {
            Err(detail) => ValueOutcome::NoVerdict { detail },
            Ok(Some(false)) => self.upper.diagnostics(value, control, true),
            Ok(Some(true)) => ValueOutcome::Satisfies,
            Ok(None) => ValueOutcome::NoVerdict {
                detail: self.unavailable.clone().unwrap_or_else(|| {
                    no_verdict(
                        NoVerdictReason::EvaluatorFailure,
                        "inconsistent-exact-bounds",
                        "the evaluator disagreed on two exact projections",
                    )
                }),
            },
        }
    }
}
fn bounded_decision(
    upper: impl FnOnce() -> bool,
    lower: impl FnOnce() -> bool,
    control: &dyn WorkControl,
) -> Result<Option<bool>, NoVerdict> {
    let upper = upper();
    control.check()?;
    if !upper {
        return Ok(Some(false));
    }
    let lower = lower();
    control.check()?;
    Ok(lower.then_some(true))
}
fn verdict_pass<T>(
    limits: &Limits,
    evaluate: impl FnOnce() -> Result<T, NoVerdict>,
) -> Result<T, NoVerdict> {
    let evaluated = backend::string_scope(|| {
        jsonschema::ob_work::bounded(limits.evaluation_steps, limits.evaluation_depth, || {
            jsonschema::ob_ecma::top_level(limits.regex_steps, evaluate)
        })
    })
    .map_err(|_| {
        no_verdict(
            NoVerdictReason::UnsupportedCapability,
            "lone-surrogate-instance",
            "a reached keyword requires Unicode scalar string interpretation",
        )
    })?;
    match evaluated {
        Ok(Ok(result)) => result,
        Ok(Err(jsonschema::ob_ecma::Exhausted::UnsupportedProperty)) => Err(no_verdict(
            NoVerdictReason::UnsupportedCapability,
            "unicode-property-matching",
            "Unicode property-escape matching is outside the qualified evaluator capability",
        )),
        Ok(Err(_)) => Err(no_verdict(
            NoVerdictReason::LimitExceeded,
            "regex-work-limit",
            "regular-expression work limit reached",
        )),
        Err(reason) => Err(evaluation_stop(reason)),
    }
}

fn check_patterns(
    resource: &SchemaResource,
    program: &EvaluationProgram,
    limits: &Limits,
    control: &dyn WorkControl,
) -> Result<(), NoVerdict> {
    // Only schema pattern positions count; opaque values are not interpreted.
    let Some(definitions) = resource.document.get("$defs").and_then(|v| v.members()) else {
        return Ok(());
    };
    for definition in definitions {
        control.check()?;
        let schema = definition.value;
        let mut patterns = Vec::new();
        if let Some(pattern) = schema.get("pattern").and_then(|v| v.as_str()) {
            patterns.push(("pattern", pattern));
        }
        if let Some(entries) = schema.get("patternProperties").and_then(|v| v.members()) {
            patterns
                .extend(entries.filter_map(|m| m.name.as_str().map(|p| ("patternProperties", p))));
        }
        for (keyword, pattern) in patterns {
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
            // Compile while the projected node still has an unambiguous source
            // identity. Dependency build errors may only carry relative paths.
            if jsonschema::ob_ecma::Regex::new(pattern).is_err() {
                let mut detail = no_verdict(
                    NoVerdictReason::ConservativePreparation,
                    "schema-pattern-compilation",
                    format!("the schema {keyword} regular expression could not be compiled"),
                );
                detail.location = program
                    .original_location(&format!(
                        "{}#/$defs/{}/{keyword}",
                        resource.uri,
                        definition.name.as_str().expect("projected definition name")
                    ))
                    .map(original_location);
                return Err(detail);
            }
        }
    }
    Ok(())
}

fn diagnostic_message(kind: &jsonschema::error::ValidationErrorKind) -> String {
    use jsonschema::error::ValidationErrorKind as K;
    let keyword_message = match kind.keyword() {
        "minimum" => Some(
            "number is below the inclusive lower bound; inspect minimum at the schema location",
        ),
        "maximum" => {
            Some("number exceeds the inclusive upper bound; inspect maximum at the schema location")
        }
        "exclusiveMinimum" => Some(
            "number must exceed the lower bound; inspect exclusiveMinimum at the schema location",
        ),
        "exclusiveMaximum" => Some(
            "number must be below the upper bound; inspect exclusiveMaximum at the schema location",
        ),
        "minLength" => Some(
            "string has fewer characters than required; inspect minLength at the schema location",
        ),
        "maxLength" => Some(
            "string exceeds the permitted character count; inspect maxLength at the schema location",
        ),
        "minItems" => {
            Some("array has fewer items than required; inspect minItems at the schema location")
        }
        "maxItems" => {
            Some("array exceeds the permitted item count; inspect maxItems at the schema location")
        }
        "minProperties" => Some(
            "object has fewer members than required; inspect minProperties at the schema location",
        ),
        "maxProperties" => Some(
            "object exceeds the permitted member count; inspect maxProperties at the schema location",
        ),
        "enum" => {
            Some("value is not one of the allowed values; inspect enum at the schema location")
        }
        "const" => {
            Some("value differs from the required constant; inspect const at the schema location")
        }
        _ => None,
    };
    if let Some(message) = keyword_message {
        return message.into();
    }
    let message = match kind {
        K::Type { kind } => {
            // The engine type enum has seven fixed names. Never render the schema
            // or rejected instance, including user-controlled enum/member values.
            use jsonschema::error::TypeKind;
            return match kind {
                TypeKind::Single(expected) => format!("expected JSON type: {expected}"),
                TypeKind::Multiple(expected) => format!(
                    "expected one of JSON types: {}",
                    expected
                        .iter()
                        .map(|ty| ty.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
        }
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
    };
    message.into()
}

fn evaluation_stop(stop: jsonschema::ob_work::Stop) -> NoVerdict {
    use jsonschema::ob_work::Stop;
    let (reason, code, message) = match stop {
        Stop::Cycle => (
            NoVerdictReason::ConservativePreparation,
            "evaluation-cycle",
            "evaluation encountered a potential cycle and could not establish a verdict",
        ),
        Stop::Arithmetic => (
            NoVerdictReason::LimitExceeded,
            "numeric-arithmetic-limit",
            "numeric arithmetic exceeds the admitted evaluation limit",
        ),
        Stop::Work | Stop::Depth => (
            NoVerdictReason::LimitExceeded,
            "evaluation-work-limit",
            "evaluation work or depth limit reached",
        ),
    };
    no_verdict(reason, code, message)
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn preparation_classification_uses_kind_metadata_and_unwraps_property_names() {
        use jsonschema::error::ValidationErrorKind;
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .should_validate_formats(true)
            .build(&serde_json::json!({"propertyNames":{"format":"regex"}}))
            .unwrap();
        let value = serde_json::json!({"[SECRET":true});
        let error = validator.validate(&value).unwrap_err();
        assert!(matches!(
            error.kind(),
            ValidationErrorKind::PropertyNames { .. }
        ));
        let classified = preparation_error(error.kind());
        assert_eq!(classified.reason, NoVerdictReason::ConservativePreparation);
        assert_eq!(classified.code, "schema-pattern-compilation");
        assert_eq!(
            classified.message,
            "a schema regular expression could not be compiled; inspect pattern and patternProperties"
        );
        assert_eq!(classified.location, None);
        for kind in [
            ValidationErrorKind::Format {
                format: "SECRET-unknown".into(),
            },
            ValidationErrorKind::Custom {
                keyword: "SECRET-keyword".into(),
                message: "SECRET-detail".repeat(1000),
            },
        ] {
            let fallback = preparation_error(&kind);
            assert_eq!(fallback.reason, NoVerdictReason::ConservativePreparation);
            assert_eq!(fallback.code, "evaluator-preparation");
            assert_eq!(
                fallback.message,
                "the evaluator could not prepare the projected schema"
            );
            assert_eq!(fallback.location, None);
        }
    }
    #[test]
    fn runtime_cycle_guard_has_a_truthful_conservative_code() {
        use jsonschema::ob_work::Stop;
        let cycle = evaluation_stop(Stop::Cycle);
        assert_eq!(cycle.reason, NoVerdictReason::ConservativePreparation);
        assert_eq!(cycle.code, "evaluation-cycle");
        for stop in [Stop::Work, Stop::Depth, Stop::Arithmetic] {
            let stopped = evaluation_stop(stop);
            assert_eq!(stopped.reason, NoVerdictReason::LimitExceeded);
            assert_ne!(stopped.code, cycle.code);
        }
    }
}

#[cfg(test)]
mod partial_budget_tests {
    use super::*;
    #[test]
    fn optional_decline_classification_is_finite_and_preserves_exact_strict_evidence() {
        let mut strict = NoVerdict::new(
            NoVerdictReason::ResourceUnavailable,
            "resource-unavailable",
            "original strict message",
        );
        strict.location = Some(SchemaLocation {
            resource: Some("https://original.invalid/R".into()),
            pointer: "/properties/long~0name".into(),
        });
        for (reason, code) in [
            (
                NoVerdictReason::ConservativePreparation,
                "partial-nonpositive-influence",
            ),
            (
                NoVerdictReason::ConservativePreparation,
                "partial-annotation-or-dynamic",
            ),
            (NoVerdictReason::LimitExceeded, "partial-program-byte-limit"),
            (NoVerdictReason::LimitExceeded, "partial-scratch-limit"),
            (NoVerdictReason::LimitExceeded, "schema-hole-limit"),
            (
                NoVerdictReason::LimitExceeded,
                "partial-generated-node-limit",
            ),
            (
                NoVerdictReason::LimitExceeded,
                "partial-generated-edge-limit",
            ),
        ] {
            let declined = no_verdict(reason, code, "irrelevant planner message");
            assert_eq!(
                serde_json::to_value(recover_partial_decline(strict.clone(), declined)).unwrap(),
                serde_json::to_value(&strict).unwrap()
            );
            let wrong_reason = no_verdict(
                NoVerdictReason::EvaluatorFailure,
                code,
                "same code, actual defect",
            );
            assert_eq!(
                recover_partial_decline(strict.clone(), wrong_reason).reason,
                NoVerdictReason::EvaluatorFailure
            );
        }
        for (reason, code) in [
            (NoVerdictReason::Cancelled, "cancelled"),
            (NoVerdictReason::LimitExceeded, "schema-node-limit"),
            (NoVerdictReason::LimitExceeded, "schema-edge-limit"),
            (NoVerdictReason::LimitExceeded, "schema-depth-limit"),
            (NoVerdictReason::LimitExceeded, "program-json-limit"),
            (
                NoVerdictReason::EvaluatorFailure,
                "projection-output-mismatch",
            ),
            (NoVerdictReason::ConservativePreparation, "invalid-schema"),
            (
                NoVerdictReason::ConservativePreparation,
                "invalid-reference-fragment",
            ),
            (NoVerdictReason::ConservativePreparation, "in-place-cycle"),
            (
                NoVerdictReason::ConservativePreparation,
                "schema-pattern-compilation",
            ),
            (NoVerdictReason::UnsupportedCapability, "schema-dialect"),
            (
                NoVerdictReason::UnsupportedCapability,
                "lone-surrogate-schema",
            ),
        ] {
            let mut actual = no_verdict(reason, code, "actual refusal");
            actual.location = Some(SchemaLocation {
                resource: Some("https://original.invalid/R".into()),
                pointer: "/selected/schema".into(),
            });
            assert_eq!(
                serde_json::to_value(recover_partial_decline(strict.clone(), actual.clone()))
                    .unwrap(),
                serde_json::to_value(actual).unwrap()
            );
        }
    }
    #[test]
    fn cancellation_between_passes_never_enters_lower() {
        let control = std::sync::atomic::AtomicBool::new(false);
        let result = bounded_decision(
            || {
                control.store(true, std::sync::atomic::Ordering::Release);
                true
            },
            || panic!("lower must not run"),
            &control,
        );
        assert_eq!(result.unwrap_err().reason, NoVerdictReason::Cancelled);
    }
    #[test]
    fn both_verdict_passes_share_work_and_an_exhausted_pass_never_decides() {
        let limits = Limits {
            evaluation_steps: 10,
            ..Limits::default()
        };
        for (upper_cost, lower_cost, upper) in [(6, 6, true), (11, 0, false), (0, 11, true)] {
            let result = verdict_pass(&limits, || {
                bounded_decision(
                    || {
                        jsonschema::ob_work::charge(upper_cost);
                        upper
                    },
                    || {
                        jsonschema::ob_work::charge(lower_cost);
                        true
                    },
                    &std::sync::atomic::AtomicBool::new(false),
                )
            });
            assert_eq!(result.unwrap_err().reason, NoVerdictReason::LimitExceeded);
        }
        assert_eq!(
            verdict_pass(&limits, || bounded_decision(
                || false,
                || panic!("failure uses no lower pass"),
                &std::sync::atomic::AtomicBool::new(false)
            ))
            .unwrap(),
            Some(false)
        );
    }
    #[test]
    fn upper_and_lower_regex_work_share_one_allowance() {
        let regex = jsonschema::ob_ecma::Regex::new("^a+$").unwrap();
        let input = "a".repeat(100);
        let needed = (0..10000)
            .find(|&n| {
                jsonschema::ob_ecma::top_level(n, || regex.is_match(&input))
                    .is_ok_and(|r| matches!(r, Ok(true)))
            })
            .unwrap();
        assert!(needed > 0);
        let limits = Limits {
            regex_steps: needed,
            ..Limits::default()
        };
        let result = verdict_pass(&limits, || {
            bounded_decision(
                || regex.is_match(&input).unwrap_or(false),
                || regex.is_match(&input).unwrap_or(false),
                &std::sync::atomic::AtomicBool::new(false),
            )
        });
        assert_eq!(result.unwrap_err().code, "regex-work-limit");
        let healthy = Limits {
            regex_steps: needed * 2,
            ..Limits::default()
        };
        assert_eq!(
            verdict_pass(&healthy, || bounded_decision(
                || regex.is_match(&input).unwrap_or(false),
                || regex.is_match(&input).unwrap_or(false),
                &std::sync::atomic::AtomicBool::new(false)
            ))
            .unwrap(),
            Some(true)
        );
    }
}

fn original_location(at: openbindings_internal_schema::Location) -> SchemaLocation {
    SchemaLocation {
        resource: at.resource,
        pointer: at.pointer,
    }
}
fn projection_failure(f: openbindings_internal_schema::Failure) -> NoVerdict {
    use openbindings_internal_schema::FailureReason as F;
    let reason = match f.reason {
        F::UnsupportedCapability => NoVerdictReason::UnsupportedCapability,
        F::ConservativePreparation => NoVerdictReason::ConservativePreparation,
        F::ResourceUnavailable => NoVerdictReason::ResourceUnavailable,
        F::LimitExceeded => NoVerdictReason::LimitExceeded,
        F::Cancelled => NoVerdictReason::Cancelled,
        F::EvaluatorFailure => NoVerdictReason::EvaluatorFailure,
    };
    NoVerdict {
        reason,
        code: f.code,
        message: f.message,
        location: f.location.map(original_location),
    }
}
