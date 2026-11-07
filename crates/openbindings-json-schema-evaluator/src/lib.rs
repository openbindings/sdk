#![doc = include_str!("../README.md")]
// A method that returns `Self` is a builder step; dropping its result loses the change.
#![warn(clippy::return_self_not_must_use)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#[cfg(test)]
extern crate self as openbindings_json_schema_evaluator;
mod compiled;
#[cfg(test)]
mod compiled_qualification;
mod es2020_properties;
mod pattern;
mod semantic;
use openbindings::*;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
/// Built-in evaluator budgets, separate from [`PreparationLimits`].
/// Preparation exhaustion returns a preparation failure; evaluation exhaustion
/// returns no verdict. Exhausted diagnostics preserve an established failure
/// with incomplete problems. These are not elapsed-time deadlines or total
/// process-memory bounds.
/// Each value invocation starts a fresh evaluation allowance; verdict and
/// diagnostic passes share what remains. Required custom-meta evaluation debits
/// its own evaluation allowance and the enclosing preparation attempt, so either
/// exhaustion can return a preparation limit.
pub struct Limits {
    /// Cooperative SDK evaluation work units. `None` (default) sets no
    /// caller-selected cap; internal capability bounds still apply. These are
    /// SDK-defined debits, not CPU operations or a count of schema steps.
    pub evaluation_sdk_work_units: Option<usize>,
    /// Maximum UTF-8 bytes in one pattern and its expanded form; default 1 MiB.
    pub pattern_bytes: usize,
    /// Caller-selected authored pattern group ceiling; default 32. The supported
    /// maximum remains 32 even when a larger value is requested.
    pub pattern_depth: usize,
    /// Normalized pattern-form extent; default 4 MiB. Original-node preparation
    /// shares equivalent values and names in one temporary construction index.
    /// Expanded forms can be larger than authored patterns.
    pub total_pattern_bytes: usize,
    /// Pattern representation footprint; default and supported maximum 64 MiB.
    /// Original-node preparation counts unique Program/Arc owners, Simple backing
    /// and NFA representation lengths, including terminal interpretations. Search
    /// caches use each value call's separate allowance. This does not measure
    /// allocator capacities, transient dependency allocations or process memory.
    pub total_pattern_memory: usize,
    /// Cumulative SDK evaluation storage reservations; default 48 MiB. This
    /// includes proof, memo, scratch, search-cache and diagnostic storage. These
    /// are SDK estimates, not allocator requests, peak live memory, immutable
    /// pattern-program size or a total process-memory bound.
    pub evaluation_sdk_reservation_bytes: usize,
    /// Reached schema-resource bound. `None` (default) sets no additional
    /// resource-count cap. Original-environment preparation still has its own
    /// footprint, work and allocation bounds through [`PreparationLimits`].
    pub schema_resources: Option<usize>,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            evaluation_sdk_work_units: None,
            pattern_bytes: 1024 * 1024,
            pattern_depth: pattern::MAX_GROUP_DEPTH,
            total_pattern_bytes: 4 * 1024 * 1024,
            total_pattern_memory: 64 * 1024 * 1024,
            evaluation_sdk_reservation_bytes: 48 * 1024 * 1024,
            schema_resources: None,
        }
    }
}
#[derive(Clone, Debug, Default)]
/// Default schema evaluator with exact JSON values and explicit external resources.
/// It performs no network or filesystem acquisition. Contained schemas use the
/// specification's fixed 2020-12 interpretation; supplied resources retain their
/// original dialect, identifiers and dynamic scope.
///
/// A bounded plan evaluates original schema locations, reasoning about validity
/// and evaluated-property/item annotations. Known undefined predicates,
/// unavailable content and evaluator limitations remain distinct. A surrounding
/// applicator may settle a result despite a local unknown; otherwise evaluation
/// returns [`ValueOutcome::Undefined`] or a typed [`ValueOutcome::NoVerdict`].
/// Recursive proofs use certified summaries and bounded coordinate refinement;
/// unresolved components may still require an unsupported-capability result.
///
/// Preparation and evaluation honor their explicit limits and cancellation.
/// [`Detail::Outcome`] avoids collecting failure explanations. [`Detail::Explain`]
/// supplies bounded problems at original source locations, with schema facts in
/// `ValueProblem::details`: missing members, expected types, exact numeric/size
/// bounds and complete enum choices. Messages stay fixed and Debug is redacted.
/// Whole-problem serialization sends schema facts; an app with a private contract
/// should forward only `instance` and `code` publicly. Facts that cannot fit or
/// be represented exactly leave diagnostics incomplete without changing the
/// outcome. A custom [`SchemaEvaluator`] can
/// provide different qualified capabilities.
pub struct DefaultEvaluator {
    limits: Limits,
}
impl DefaultEvaluator {
    /// Construct the companion with [`Limits::default`]; no schemas are prepared or resources acquired yet.
    pub fn new() -> Self {
        Self::default()
    }
    /// Construct the companion with explicit limits; they apply during preparation/evaluation, not as wall-clock deadlines.
    #[must_use]
    pub fn with_limits(limits: Limits) -> Self {
        Self { limits }
    }
    /// Borrow this evaluator's configured limits without allocation.
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
fn no_verdict(
    reason: NoVerdictReason,
    code: impl Into<std::borrow::Cow<'static, str>>,
    message: impl Into<std::borrow::Cow<'static, str>>,
) -> NoVerdict {
    NoVerdict::new(reason, code, message)
}
/// Opaque owned validator produced by [`DefaultEvaluator`]. Clones share its
/// immutable prepared state; no public evaluator trait requires this ownership.
#[derive(Clone)]
pub struct DefaultValidator {
    plan: Arc<semantic::Plan>,
    compiled: compiled::Lane,
}
impl std::fmt::Debug for DefaultValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultValidator").finish_non_exhaustive()
    }
}
impl SchemaValidator for DefaultValidator {
    fn evaluate(
        &self,
        value: &JsonValue,
        detail: Detail,
        control: &dyn WorkControl,
    ) -> ValueOutcome {
        if detail == Detail::Outcome
            && let Some(outcome) = self.compiled.outcome(value, control)
        {
            return outcome;
        }
        self.plan.evaluate(value, detail, control)
    }
}
impl SchemaEvaluator for DefaultEvaluator {
    type Validator = DefaultValidator;
    fn prepare(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
    ) -> Result<Self::Validator, PreparationFailure> {
        openbindings_internal_schema::preparation::Attempt::run(
            limits.max_sdk_work_units,
            limits.max_sdk_reservation_bytes,
            &|| control.is_cancelled(),
            |attempt| self.prepare_attempt(request, limits, control, attempt),
        )
        .map_err(PreparationFailure::from)?
    }
}
impl DefaultEvaluator {
    fn prepare_attempt(
        &self,
        request: EvaluationRequest<'_>,
        limits: &PreparationLimits,
        control: &dyn WorkControl,
        attempt: &dyn openbindings_internal_schema::Control,
    ) -> Result<DefaultValidator, PreparationFailure> {
        use openbindings_internal_schema::preparation::EnvironmentRequest;
        control.check()?;
        let environment = request.environment_with_control(
            limits.max_schema_nodes,
            limits.max_identity_bytes,
            attempt,
        )?;
        let (plan, compiled) = semantic::Plan::with_compiled(
            environment,
            self.limits.clone(),
            *limits,
            attempt,
            control,
        )?;
        control.check()?;
        Ok(DefaultValidator {
            plan: Arc::new(plan),
            compiled,
        })
    }
}
// Why a pattern `keyword` holds is not matched.
fn unsupported_pattern(unsupported: pattern::Unsupported, _keyword: &str) -> NoVerdict {
    match unsupported {
        pattern::Unsupported::Backtracking => no_verdict(
            NoVerdictReason::UnsupportedCapability,
            "pattern-backtracking",
            "the schema pattern needs a backtracking engine, which this evaluator does not run",
        ),
        pattern::Unsupported::PropertyEscape => no_verdict(
            NoVerdictReason::UnsupportedCapability,
            "unicode-property-matching",
            "the valid Unicode property is absent from the pinned engine tables",
        ),
        pattern::Unsupported::NonWordBoundary => no_verdict(
            NoVerdictReason::UnsupportedCapability,
            "pattern-non-word-boundary",
            "the schema pattern's non-word-boundary assertion is outside the qualified evaluator capability",
        ),
        pattern::Unsupported::Size => no_verdict(
            NoVerdictReason::LimitExceeded,
            "pattern-size-limit",
            "a schema regular expression exceeds the engine's size limits",
        ),
    }
}

#[cfg(test)]
#[path = "../tests/private/resource_identity.rs"]
mod resource_identity;
#[cfg(test)]
#[path = "../tests/private/supplied_reuse.rs"]
mod supplied_reuse;
