//! Optional JSON Schema 2020-12 evaluation. Resources are explicit; no I/O.
#![forbid(unsafe_code)]
#![warn(missing_docs)]
mod details;
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
/// Finite default-evaluator budgets. Work counts are implementation units, not milliseconds; exhaustion yields no verdict unless an established failure only loses diagnostic completeness. Zero is literal, except `max_problems` has a minimum count allowance of one. A byte budget may admit no complete problems.
pub struct Limits {
    /// Evaluation work units per verdict/diagnostic pass; default 2,000,000. A partial verdict shares this allowance across upper and lower evaluations.
    pub evaluation_steps: usize,
    /// Nested evaluation depth; default 1024, distinct from source JSON nesting.
    pub evaluation_depth: usize,
    /// Regular-expression work/backtracking budget per verdict/diagnostic pass; default 2,000,000. Upper and lower share one verdict allowance.
    pub regex_steps: usize,
    /// Maximum retained failure diagnostics; default 256, effective minimum count allowance one; byte admission may retain none. Truncation sets `problems_complete` false without changing an established failure.
    pub max_problems: usize,
    /// Aggregate retained UTF-8 diagnostic string bytes per failure result; default 1 MiB. Includes pointers, resource identifiers, codes, messages and requested detail strings. Zero preserves failure with empty, incomplete diagnostics. Not a heap or serialized byte limit.
    pub diagnostic_bytes: usize,
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
            max_problems: 256,
            diagnostic_bytes: 1024 * 1024,
            compile_json_depth: 512,
            pattern_bytes: 1024 * 1024,
            pattern_depth: 256,
        }
    }
}
#[derive(Clone, Debug, Default)]
/// Optional JSON Schema 2020-12 companion with exact numeric semantics and explicit resources only. It performs no network/filesystem acquisition. Format is annotation, Unicode property-escape matching is not qualified, and potential non-progressing cycles can conservatively refuse. Use a custom [`SchemaEvaluator`] when different qualified capabilities are required.
pub struct DefaultEvaluator {
    limits: Limits,
    include_schema_details: bool,
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
            include_schema_details: false,
        }
    }
    /// Opt into source-controlled schema facts in failure diagnostics (default false).
    /// Prepared validators retain their original document/resource snapshots only
    /// when enabled. Facts may contain secrets; disclose them only in an appropriate
    /// application context. Exact numeric operands remain JSON token strings.
    /// The diagnostic byte budget admits complete facts or an explicit Truncated
    /// marker; it never clips a name/token or returns a partial enum array.
    pub fn with_schema_details(mut self, include: bool) -> Self {
        self.include_schema_details = include;
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
        control: &WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        control.check()?;
        match request.evaluation_program(control) {
            Ok(program) => Ok(Arc::new(self.compile(program, request, control)?)),
            Err(detail)
                if detail.reason == NoVerdictReason::ResourceUnavailable
                    && detail.code == "resource-unavailable" =>
            {
                let bounds = request
                    .evaluation_bounds(control)
                    .map_err(|decline| recover_partial_decline(detail, decline))?;
                let (lower, upper, unavailable) = bounds.into_parts();
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
// Direct SchemaRequest::evaluation_bounds callers still receive planner errors.
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
        control: &WorkControl,
    ) -> Result<Compiled, NoVerdict> {
        let mut registry = jsonschema::Registry::new().retriever(NoRetrieval);
        let mut literals = literals::Literals::default();
        for resource in &program.resources {
            control.check()?;
            check_patterns(resource.document.view(), &self.limits, control)?;
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
            .apply(numeric::apply(jsonschema::options_for::<FlatJson>()))
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
            detail_source: self.include_schema_details.then(|| request.clone()),
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
    fn resource_completeness(&self) -> ResourceCompleteness<'_> {
        ResourceCompleteness::Complete
    }
    fn validate(&self, value: &JsonValue, control: &WorkControl) -> ValueOutcome {
        if let Err(detail) = admit_instance(value, control) {
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
        control: &WorkControl,
        original_only: bool,
    ) -> ValueOutcome {
        let mut problems = Vec::new();
        let mut complete = true;
        let mut remaining = self.limits.diagnostic_bytes;
        if remaining == 0 {
            return ValueOutcome::Fails {
                problems,
                problems_complete: false,
            };
        }
        let count = self.limits.max_problems.max(1);
        let (diagnostics, usage) = jsonschema::ob_work::diagnostic_schema_details(
            self.detail_source.is_some(),
            || {
                jsonschema::ob_work::diagnostic_metadata(
                    self.limits.diagnostic_bytes,
                    count.saturating_mul(8),
                    || {
                        jsonschema::ob_work::diagnostics(count.saturating_mul(8), || {
                            jsonschema::ob_work::bounded(
                                self.limits.evaluation_steps,
                                self.limits.evaluation_depth,
                                || {
                                    jsonschema::ob_ecma::top_level(self.limits.regex_steps, || {
                                        for error in
                                            self.validator.iter_errors(backend::view(value))
                                        {
                                            if problems.len() >= count {
                                                complete = false;
                                                break;
                                            }
                                            // Messages use only fixed keyword/type metadata (<=192 bytes).
                                            // No source-controlled operand or rejected instance is rendered.
                                            let message = if original_only
                                                && error.kind().keyword() == "oneOf"
                                            {
                                                "value does not satisfy exactly one alternative"
                                                    .into()
                                            } else {
                                                diagnostic_message(error.kind())
                                            };
                                            let code = error.kind().keyword();
                                            let reserve = if self.detail_source.is_some() {
                                                details::TRUNCATED_BYTES
                                            } else {
                                                0
                                            };
                                            let fixed = code
                                                .len()
                                                .saturating_add(message.len())
                                                .saturating_add(reserve);
                                            if fixed > remaining {
                                                complete = false;
                                                break;
                                            }
                                            let all_paths = diagnostic_paths(
                                                &error,
                                                value,
                                                |root, child| {
                                                    if problems.len() >= count {
                                                        return false;
                                                    }
                                                    let bytes = root.len().saturating_add(
                                                        child.map_or(0, |s| {
                                                            1usize.saturating_add(escaped_len(s))
                                                        }),
                                                    );
                                                    if fixed.saturating_add(bytes) > remaining {
                                                        return false;
                                                    }
                                                    let location =
                                                        match error.absolute_keyword_location() {
                                                            Some(uri) => match self
                                                                .program
                                                                .original_location_bounded(
                                                                    uri.as_str(),
                                                                    remaining - fixed - bytes,
                                                                ) {
                                                                Ok(location) => location,
                                                                Err(LocationBudgetExceeded) => {
                                                                    return false;
                                                                }
                                                            },
                                                            None => None,
                                                        };
                                                    if original_only && location.is_none() {
                                                        // Keep the offered outer error boundary. Never
                                                        // flatten generated contexts to recover leaves.
                                                        complete = false;
                                                        return true;
                                                    }
                                                    let location_bytes =
                                                        location.as_ref().map_or(0, |at| {
                                                            at.pointer.len().saturating_add(
                                                                at.resource
                                                                    .as_ref()
                                                                    .map_or(0, String::len),
                                                            )
                                                        });
                                                    let total = fixed + bytes + location_bytes;
                                                    let (detail, detail_bytes) = if let Some(
                                                        source,
                                                    ) =
                                                        &self.detail_source
                                                    {
                                                        match details::collect(
                                                            source,
                                                            location.as_ref(),
                                                            error.kind(),
                                                            remaining - total + reserve,
                                                            control,
                                                        ) {
                                                            details::Collection::Available(
                                                                detail,
                                                                bytes,
                                                            ) => (Some(detail), bytes.max(reserve)),
                                                            details::Collection::Truncated => {
                                                                complete = false;
                                                                (Some(ValueProblemDetails::Truncated), reserve)
                                                            }
                                                            details::Collection::Unavailable => {
                                                                (None, reserve)
                                                            }
                                                            details::Collection::Interrupted => {
                                                                return false;
                                                            }
                                                        }
                                                    } else {
                                                        (None, 0)
                                                    };
                                                    let total = total + detail_bytes - reserve;
                                                    // Allocate only a whole admitted problem, with exact capacity.
                                                    let mut path = String::with_capacity(bytes);
                                                    path.push_str(root);
                                                    if let Some(child) = child {
                                                        path.push('/');
                                                        for character in child.chars() {
                                                            match character {
                                                                '~' => path.push_str("~0"),
                                                                '/' => path.push_str("~1"),
                                                                _ => path.push(character),
                                                            }
                                                        }
                                                    }
                                                    remaining -= total;
                                                    problems.push(ValueProblem {
                                                        instance_pointer: path,
                                                        schema_location: location,
                                                        code: code.into(),
                                                        message: message.clone(),
                                                        details: detail,
                                                    });
                                                    true
                                                },
                                            );
                                            if !all_paths || control.is_cancelled() {
                                                complete = false;
                                                break;
                                            }
                                        }
                                    })
                                },
                            )
                        })
                    },
                )
            },
        );
        if !matches!(diagnostics, Ok(Ok(Ok(()))))
            || usage.rejected_copies != 0
            || usage.collection_truncated
        {
            complete = false;
        }
        if let Err(detail) = control.check() {
            return ValueOutcome::NoVerdict { detail };
        }
        ValueOutcome::Fails {
            problems,
            problems_complete: complete,
        }
    }
}
struct PartialCompiled {
    upper: Compiled,
    lower: Compiled,
    unavailable: Option<NoVerdict>,
}
impl PreparedSchema for PartialCompiled {
    fn resource_completeness(&self) -> ResourceCompleteness<'_> {
        self.unavailable.as_ref().map_or(
            ResourceCompleteness::Complete,
            ResourceCompleteness::incomplete,
        )
    }
    fn validate(&self, value: &JsonValue, control: &WorkControl) -> ValueOutcome {
        if let Err(detail) = admit_instance(value, control) {
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
    control: &WorkControl,
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
fn admit_instance(value: &JsonValue, control: &WorkControl) -> Result<(), NoVerdict> {
    control.check()?;
    if backend::has_unpaired(value) {
        return Err(no_verdict(
            NoVerdictReason::UnsupportedCapability,
            "lone-surrogate-instance",
            "instance interpretation requires Unicode scalar strings",
        ));
    }
    Ok(())
}
fn verdict_pass<T>(
    limits: &Limits,
    evaluate: impl FnOnce() -> Result<T, NoVerdict>,
) -> Result<T, NoVerdict> {
    match jsonschema::ob_work::bounded(limits.evaluation_steps, limits.evaluation_depth, || {
        jsonschema::ob_ecma::top_level(limits.regex_steps, evaluate)
    }) {
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

fn escaped_len(text: &str) -> usize {
    text.len()
        .saturating_add(text.bytes().filter(|b| matches!(b, b'~' | b'/')).count())
}
// Visit one path at a time, borrowing root/member strings. Never expand the whole
// vendor keyword collection or stringify an input value to find an item index.
fn diagnostic_paths(
    error: &jsonschema::ValidationError<'_>,
    value: &JsonValue,
    mut admit: impl FnMut(&str, Option<&str>) -> bool,
) -> bool {
    use jsonschema::error::ValidationErrorKind as Kind;
    let root = error.instance_path().as_str();
    match error.kind() {
        Kind::AdditionalProperties { unexpected } | Kind::UnevaluatedProperties { unexpected } => {
            unexpected.iter().all(|name| admit(root, Some(name)))
        }
        Kind::AdditionalItems { limit } => {
            match value.at(root).and_then(|v| v.elements().map(|a| a.len())) {
                Some(len) => (*limit..len).all(|i| admit(root, Some(&i.to_string()))),
                None => admit(root, None),
            }
        }
        Kind::UnevaluatedItems { indexes, .. } if !indexes.is_empty() => {
            indexes.iter().all(|i| admit(root, Some(&i.to_string())))
        }
        _ => admit(root, None),
    }
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
        Stop::Work | Stop::Depth | Stop::Diagnostics => (
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
        for stop in [Stop::Work, Stop::Depth, Stop::Diagnostics, Stop::Arithmetic] {
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
        let control = WorkControl::new();
        let result = bounded_decision(
            || {
                control.cancel();
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
                    &WorkControl::new(),
                )
            });
            assert_eq!(result.unwrap_err().reason, NoVerdictReason::LimitExceeded);
        }
        assert_eq!(
            verdict_pass(&limits, || bounded_decision(
                || false,
                || panic!("failure uses no lower pass"),
                &WorkControl::new()
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
                &WorkControl::new(),
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
                &WorkControl::new()
            ))
            .unwrap(),
            Some(true)
        );
    }
}
