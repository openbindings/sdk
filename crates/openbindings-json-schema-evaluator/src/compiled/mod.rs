//! Private restoration of the18a66957 compiled value validator.
//! Experimental selection is owned by the parent; no public legacy API is exposed.
mod admission;
mod bundle;
mod depth;
mod expansion;
mod operands;
mod pass;
mod pattern;
mod recursion;
#[cfg(test)]
pub(super) fn without_projection_compaction<T>(run: impl FnOnce() -> T) -> T {
    bundle::without_compaction(run)
}
pub(super) const MAXIMUM_NODES: usize = 100_000;
// A pre-entry work estimate, independent of scratch and caller work controls.
// 10 million lost a valid source-puller verdict. 40 million keeps the corpus
// maximum coefficient (39) compiled at the default million-node parser limit.
pub(super) const MAXIMUM_EVALUATION_APPLICATIONS: usize = 40_000_000;
#[cfg(test)]
pub(super) mod entries {
    use std::cell::Cell;
    thread_local! {
        static COUNTS: Cell<[usize; 3]> = const { Cell::new([0; 3]) };
        static EXPANSION: Cell<Option<usize>> = const { Cell::new(None) };
        static APPLICATIONS: Cell<Option<usize>> = const { Cell::new(None) };
    }
    pub(crate) fn record(stage: usize) {
        COUNTS.with(|counts| {
            let mut value = counts.get();
            value[stage] += 1;
            counts.set(value);
        });
    }
    pub(crate) fn take() -> [usize; 3] {
        COUNTS.with(|counts| counts.replace([0; 3]))
    }
    pub(crate) fn record_expansion(applications: usize) {
        EXPANSION.with(|count| count.set(Some(applications)));
    }
    pub(crate) fn take_expansion() -> Option<usize> {
        EXPANSION.with(|count| count.replace(None))
    }
    pub(crate) fn record_applications(applications: usize) {
        APPLICATIONS.with(|count| count.set(Some(applications)));
    }
    pub(crate) fn take_applications() -> Option<usize> {
        APPLICATIONS.with(|count| count.replace(None))
    }
}
use openbindings::*;
use openbindings_internal_json::backend::{self, FlatJson};
use openbindings_internal_schema::Definitions;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};
#[derive(Clone, Debug)]
struct Limits {
    compile_json_depth: usize,
    total_pattern_bytes: usize,
    total_pattern_memory: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            compile_json_depth: 512,
            total_pattern_bytes: 4 * 1024 * 1024,
            total_pattern_memory: 64 * 1024 * 1024,
        }
    }
}
struct Compiler {
    limits: Limits,
    readings: BTreeMap<Arc<str>, pattern::Reading>,
}
struct ConstructionControl<'a> {
    caller: &'a dyn WorkControl,
    attempt: &'a dyn openbindings_internal_schema::Control,
    cancelled: std::sync::atomic::AtomicBool,
}
impl WorkControl for ConstructionControl<'_> {
    fn is_cancelled(&self) -> bool {
        use std::sync::atomic::Ordering;
        if self.cancelled.load(Ordering::Relaxed) {
            return true;
        }
        if self.caller.is_cancelled() {
            self.cancelled.store(true, Ordering::Relaxed);
            return true;
        }
        self.attempt.check().is_err()
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
    NoVerdict::new(reason, code.to_owned(), message.into())
}

fn inherited_stop() -> Option<NoVerdict> {
    use openbindings_internal_json::work::{Stop, stopped};
    stopped().map(|stop| match stop {
        Stop::Work => no_verdict(
            NoVerdictReason::LimitExceeded,
            "evaluation-sdk-work-units",
            "value evaluation exhausted its enclosing SDK work allowance",
        ),
        Stop::Stack => no_verdict(
            NoVerdictReason::LimitExceeded,
            "evaluation-depth-limit",
            "value evaluation reached its enclosing stack guard",
        ),
    })
}

fn preparation_detail(detail: NoVerdict) -> PreparationFailure {
    let kind = match detail.reason {
        NoVerdictReason::Cancelled => PreparationFailureKind::Cancelled,
        NoVerdictReason::LimitExceeded => PreparationFailureKind::Limit {
            resource: detail.code.into_owned(),
        },
        NoVerdictReason::UnsupportedCapability | NoVerdictReason::ConservativePreparation => {
            PreparationFailureKind::UnsupportedCapability {
                capability: detail.code.into_owned(),
            }
        }
        _ => PreparationFailureKind::Implementation {
            message: detail.message.into_owned(),
        },
    };
    let mut failure = PreparationFailure::new(kind);
    failure.origin = detail.location;
    failure
}

/// An experimental private choice, established before projection/backend entry.
#[derive(Clone)]
pub(super) enum Lane {
    Original(
        #[cfg_attr(
            not(test),
            expect(dead_code, reason = "qualification retains pre-entry route reasons")
        )]
        &'static str,
    ),
    Compiled(Arc<Prepared>),
}
pub(super) struct Prepared {
    inner: Arc<Compiled>,
    features: admission::Features,
    reservation: usize,
}
impl Lane {
    #[cfg(test)]
    pub(super) fn qualified_depth(&self) -> Option<usize> {
        match self {
            Self::Original(_) => None,
            Self::Compiled(prepared) => prepared.inner.deepest,
        }
    }
    pub(super) fn prepare(
        plan: &crate::semantic::Plan,
        limits: &crate::Limits,
        preparation: &PreparationLimits,
        control: &dyn WorkControl,
        attempt: &dyn openbindings_internal_schema::Control,
        reading: &dyn Fn(&str) -> Result<Option<crate::pattern::Shared>, PreparationFailure>,
    ) -> Result<Self, PreparationFailure> {
        use openbindings_internal_schema::OriginalEnvironment;
        let (mut features, reach) =
            match admission::select(plan, limits, preparation, attempt, reading)? {
                Err(reason) => return Ok(Self::Original(reason)),
                Ok(admission::Admitted { features, reach }) => (features, reach),
            };
        attempt.check().map_err(PreparationFailure::from)?;
        control.check()?;
        let environment = plan.compiled_environment();
        let space = environment.space();
        #[cfg(test)]
        entries::record(0);
        let definitions =
            space.program_definitions_reached(environment.entry(), &reach, attempt)?;
        drop(reach); // Lowering owns its output; construction no longer needs the map.
        let compiler = Compiler {
            limits: Limits::default(),
            readings: std::mem::take(&mut features.readings),
        };
        let bridge = ConstructionControl {
            caller: control,
            attempt,
            cancelled: std::sync::atomic::AtomicBool::new(false),
        };
        let result = compiler.prepared(definitions, &bridge);
        // Preserve the shared cause before translating the private constructor's
        // stop carrier. Resource exhaustion is never relabelled cancellation.
        let completion = bridge.check();
        attempt.check().map_err(PreparationFailure::from)?;
        completion?;
        control.check()?;
        attempt.check().map_err(PreparationFailure::from)?;
        let inner = result.map_err(preparation_detail)?;
        Ok(Self::Compiled(Arc::new(Prepared {
            inner,
            features,
            reservation: limits.evaluation_sdk_reservation_bytes,
        })))
    }

    /// None means value-level selection chose Plan before calling is_valid.
    pub(super) fn outcome(
        &self,
        value: &JsonValue,
        control: &dyn WorkControl,
    ) -> Option<ValueOutcome> {
        let Self::Compiled(prepared) = self else {
            return None;
        };
        if let Err(detail) = control.check() {
            return Some(ValueOutcome::NoVerdict {
                detail: detail.into(),
            });
        }
        if let Some(detail) = inherited_stop() {
            return Some(ValueOutcome::NoVerdict { detail });
        }
        if backend::has_unpaired(value)
            || value.has_duplicate_names()
            || !prepared.inner.nests_within(value, prepared.inner.deepest)
        {
            return None;
        }
        let features = &prepared.features;
        // Root node_count is constant time. Saturation also declines overflow
        // on wasm32. Keep this estimate before any synchronous backend work,
        // after the existing cancellation and inherited-stop precedence.
        if features
            .applications
            .saturating_mul(backend::node_count(value.view()))
            > MAXIMUM_EVALUATION_APPLICATIONS
        {
            return None;
        }
        if features.recursive && features.integer {
            // Integer typing is separate from allocation-free scalar literals.
            // Plain integer tokens use the backend's allocation-free lexical
            // test, including arbitrarily long integers and -0. Decimal and
            // exponent interpretation remains on Plan before backend entry.
            for token in backend::number_nodes(value) {
                if let Err(detail) = control.check() {
                    return Some(ValueOutcome::NoVerdict {
                        detail: detail.into(),
                    });
                }
                if token.is_some_and(|n| n.bytes().any(|b| matches!(b, b'.' | b'e' | b'E'))) {
                    return None;
                }
            }
        }
        if features.multiple {
            for token in backend::number_nodes(value) {
                if let Err(detail) = control.check() {
                    return Some(ValueOutcome::NoVerdict {
                        detail: detail.into(),
                    });
                }
                if token.is_some_and(|token| {
                    !admission::multiple_token(token) || !admission::inline_multiple_token(token)
                }) {
                    return None;
                }
            }
        }
        if features.unique {
            // Composite collision verification can allocate once per compared
            // pair. This initial lane admits only a root array of scalar values
            // whose exact numeric representation is inline; Plan covers the
            // larger domain without a quadratic scratch under-estimate.
            let mut elements = value.view().elements()?;
            let mut cancelled = false;
            if !elements.all(|item| {
                if control.check().is_err() {
                    cancelled = true;
                    return false;
                }
                match item.kind() {
                    JsonKind::Array | JsonKind::Object => false,
                    JsonKind::Number => item.number_text().is_some_and(|token| {
                        admission::multiple_token(token) && admission::inline_multiple_token(token)
                    }),
                    _ => true,
                }
            }) {
                if cancelled {
                    return Some(ValueOutcome::NoVerdict {
                        detail: Cancelled.into(),
                    });
                }
                return None;
            }
        }
        if features.scratch && (!features.recursive || !features.recursive_allocation_free) {
            let value_nodes = backend::node_count(value.view());
            // A schema operand below an advancing application can be used
            // at every value occurrence, including a wide bound whose
            // exponent scratch is larger than the inspected scalar.
            let operand_visits = if features.advancing { value_nodes } else { 1 };
            let bytes = value
                .bytes()
                .len()
                .saturating_add(features.literal_bytes.saturating_mul(operand_visits));
            let nodes =
                value_nodes.saturating_add(features.literal_nodes.saturating_mul(operand_visits));
            let reservation = bytes
                .saturating_mul(16)
                .saturating_add(nodes.saturating_mul(256))
                .saturating_mul(features.multiplier)
                .saturating_add(512);
            if reservation > prepared.reservation {
                // Integer typing and i64 bounds need no exact decimal scratch
                // when every number token fits i64, including within composite
                // values. Only a value over the estimate pays for this scan.
                match prepared.allocation_free(value, control).and_then(|free| {
                    if free {
                        Ok(true)
                    } else {
                        prepared.decimal_peak_fits(value, control)
                    }
                }) {
                    Ok(true) => {}
                    Ok(false) => return None,
                    Err(detail) => {
                        return Some(ValueOutcome::NoVerdict {
                            detail: detail.into(),
                        });
                    }
                }
            }
        }
        #[cfg(test)]
        entries::record(2);
        let result = prepared
            .inner
            .pass(|| prepared.inner.validator.is_valid(backend::view(value)));
        let valid = match result {
            Ok(valid) => valid,
            Err(detail) => return Some(ValueOutcome::NoVerdict { detail }),
        };
        if let Err(detail) = control.check() {
            return Some(ValueOutcome::NoVerdict {
                detail: detail.into(),
            });
        }
        Some(if valid {
            ValueOutcome::Satisfies
        } else {
            ValueOutcome::Fails {
                message: "value does not satisfy the schema".into(),
                problems: Vec::new(),
                problems_complete: false,
            }
        })
    }
}
impl Prepared {
    fn decimal_peak_fits(
        &self,
        value: &JsonValue,
        control: &dyn WorkControl,
    ) -> Result<bool, Cancelled> {
        // The lifetime proof covers only acyclic bounds and integer typing.
        // Preserve every existing admission for division, equality, uniqueness
        // and recursion, including the earlier allocation-free exemptions.
        if self.features.recursive
            || self.features.multiple
            || self.features.literals
            || self.features.unique
        {
            return Ok(false);
        }
        let mut longest = 0;
        for token in backend::number_nodes(value) {
            control.check()?;
            if let Some(token) = token {
                longest = longest.max(token.len());
                if decimal_peak(longest, self.features.longest_comparison_bound) > self.reservation
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn allocation_free(
        &self,
        value: &JsonValue,
        control: &dyn WorkControl,
    ) -> Result<bool, Cancelled> {
        if self.features.literals || self.features.noninteger_bound {
            return Ok(false);
        }
        if !matches!(value.kind(), JsonKind::Array | JsonKind::Object) {
            return Ok(value
                .view()
                .number_text()
                .is_none_or(|token| token.parse::<i64>().is_ok()));
        }
        // Preserve the existing scalar exemption for multipleOf. Composite
        // multipleOf values keep their previous reservation and domain checks.
        if self.features.multiple {
            return Ok(false);
        }
        for token in backend::number_nodes(value) {
            control.check()?;
            if token.is_some_and(|token| token.parse::<i64>().is_err()) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

// ExactBound borrows the prepared bound and drops each value Decimal before
// returning. For token lengths V and B, Decimal::read_wide holds at most V
// coefficient bytes. Power::offset holds the old exponent, a working copy and
// its boxed result (including carry growth); Wide::compare also holds the
// first adjusted exponent while building the second. Four copies of V+B plus
// 512 bytes cover those overlapping buffers, realloc transitions, the Wide
// box and machine-sized formatted offsets. Integer classification only parses
// one value. See tools/decimal-scratch/README.md for the code-derived bound.
pub(super) fn decimal_peak(value_bytes: usize, bound_bytes: usize) -> usize {
    value_bytes
        .saturating_add(bound_bytes)
        .saturating_mul(4)
        .saturating_add(512)
}
// Only the closed value program is lowered here. Original admission,
// reference resolution and diagnostics belong to the shared Plan facts.
impl Compiler {
    fn prepared(
        &self,
        projected: Definitions<'_>,
        control: &dyn WorkControl,
    ) -> Result<Arc<Compiled>, NoVerdict> {
        if projected.in_place_cycle {
            return Err(program_failure());
        }
        let mut operands = operands::Operands::default();
        let mut sources = operands::Sources::new();
        let mut resources = Vec::with_capacity(projected.resources.len());
        let mut entry = None;
        for resource in projected.resources {
            control.check()?;
            let definitions = operands.definitions(
                resource.definitions,
                &projected.literals,
                &self.limits,
                &mut sources,
                control,
            )?;
            if resource.uri == projected.entry_uri {
                entry = resource.entry;
            }
            resources.push(bundle::Resource { definitions });
        }
        let mut kept = Kept::default();
        for resource in &mut resources {
            for (name, schema) in &mut resource.definitions {
                control.check()?;
                let source = if sources.is_empty() {
                    None
                } else {
                    let number = name[1..].parse::<u32>().map_err(|_| program_failure())?;
                    sources.remove(&number)
                };
                patterns(
                    schema,
                    source,
                    &self.limits,
                    &self.readings,
                    &mut operands,
                    &mut kept,
                )
                .map_err(|(_, detail)| detail)?;
            }
        }
        let memory = pattern::Memory::new(
            self.limits.total_pattern_memory,
            kept.compiled,
            kept.matched,
        );
        drop(kept);
        let bundle = bundle::bundle(resources, &entry.ok_or_else(program_failure)?)
            .map_err(|_| program_failure())?;
        let deepest = depth::bound(&bundle.definitions, bundle.entry, 2)
            .map_err(|_| program_failure())?
            .deepest(1024);
        control.check()?;
        let document = bundle.document(control)?;
        let first = memory.first();
        let keywords = operands.keywords(memory);
        keywords.patterns().complete(control)?;
        let options = keywords
            .register(jsonschema::options_for::<FlatJson>())
            .with_draft(jsonschema::Draft::Draft202012)
            .with_retriever(NoRetrieval)
            .should_validate_formats(false)
            .with_pattern_options(pattern_options(first));
        control.check()?;
        #[cfg(test)]
        entries::record(1);
        let built = options.build(&document);
        control.check()?;
        let validator = match built {
            Ok(validator) => validator,
            Err(error) if exceeds_size(&error) => {
                share_names(options, first, &document, keywords.patterns(), control)?
            }
            Err(error) => return Err(preparation_error(&error)),
        };
        control.check()?;
        Ok(Arc::new(Compiled { validator, deepest }))
    }
}
fn program_failure() -> NoVerdict {
    no_verdict(
        NoVerdictReason::EvaluatorFailure,
        "program-shape",
        "the projected program has an unexpected shape",
    )
}
fn patterns<'a>(
    schema: &mut Value,
    source: Option<std::borrow::Cow<'a, str>>,
    limits: &Limits,
    readings: &BTreeMap<Arc<str>, pattern::Reading>,
    operands: &mut operands::Operands,
    kept: &mut Kept<'a>,
) -> Result<(), (&'static str, NoVerdict)> {
    let Value::Object(schema) = schema else {
        return Ok(());
    };
    // Count each form the evaluator keeps as it is written, and the memory
    // of each it compiles or matches with a run matcher, so preparation stops
    // at the first that passes either total.
    let mut count = |keyword: &'static str, form: &str, matcher: Option<&pattern::Simple>| {
        kept.bytes = kept.bytes.saturating_add(form.len());
        if kept.bytes > limits.total_pattern_bytes {
            return Err((
                keyword,
                no_verdict(
                    NoVerdictReason::LimitExceeded,
                    "pattern-total-byte-limit",
                    "the forms of the value schema's patterns exceed the admitted total byte limit",
                ),
            ));
        }
        match matcher {
            Some(matcher) => {
                kept.matched = kept
                    .matched
                    .saturating_add(operands::run_matcher_bytes(matcher));
            }
            None => kept.compiled += 1,
        }
        let total = limits.total_pattern_memory;
        if pattern::Memory::most_patterns(total, kept.matched)
            .is_none_or(|most| kept.compiled > most)
        {
            return Err((keyword, pattern_memory_limit()));
        }
        Ok(())
    };
    if let Some(source) = source
        && let Some(slot) = schema.get_mut("pattern")
    {
        let key = match kept.read.get(&*source) {
            Some(key) => key.clone(),
            None => {
                let reading = readings
                    .get(source.as_ref())
                    .cloned()
                    .ok_or_else(program_failure)
                    .map_err(|detail| ("pattern", detail))?;
                // Patterns with the same form share one matcher, which keeps
                // the form and compiles it unless the run matcher matches it.
                if !operands.has_pattern(&reading.form) {
                    count("pattern", &reading.form, reading.simple.as_deref())?;
                }
                let key = operands.pattern(reading);
                kept.read.insert(source, key.clone());
                key
            }
        };
        *slot = Value::String(key);
    }
    // The library matches these names itself, so the bundle keeps each form.
    if let Some(Value::Object(names)) = schema.get_mut("patternProperties") {
        let original = std::mem::take(names);
        for (source, subschema) in original {
            let refuse = |detail| ("patternProperties", detail);
            let mut form = readings
                .get(source.as_str())
                .ok_or_else(program_failure)
                .map_err(refuse)?
                .form
                .to_string();
            // Two names may share a form; an empty group keeps them apart.
            while names.contains_key(&form) {
                form.push_str("(?:)");
            }
            count("patternProperties", &form, None)?;
            form.shrink_to_fit();
            names.insert(form, subschema);
        }
    }
    Ok(())
}
// What preparation has kept for a contract's patterns so far.
#[derive(Default)]
struct Kept<'a> {
    // The bytes of the forms kept.
    bytes: usize,
    // The forms to be compiled: each distinct `pattern` form without a run
    // matcher and each `patternProperties` name.
    compiled: usize,
    // The bytes the run matchers of the other distinct `pattern` forms hold.
    matched: usize,
    // The value written for each `pattern` already read, by its source, so a
    // repeated pattern is read once.
    read: BTreeMap<std::borrow::Cow<'a, str>, String>,
}
fn unsupported_pattern(reason: pattern::Unsupported, _keyword: &str) -> NoVerdict {
    match reason {
        pattern::Unsupported::Size => no_verdict(
            NoVerdictReason::LimitExceeded,
            "pattern-size-limit",
            "a schema regular expression exceeds the engine's size limits",
        ),
        pattern::Unsupported::Memory => pattern_memory_limit(),
    }
}
// The refusal reaches callers as `PreparationFailureKind::Limit` with this code
// alone, so its message names the limit without restating the arithmetic.
fn pattern_memory_limit() -> NoVerdict {
    no_verdict(
        NoVerdictReason::LimitExceeded,
        "pattern-total-memory-limit",
        "the value schema's patterns exceed the admitted total pattern memory",
    )
}
// Whether the library refused a schema regular expression. Every pattern was
// read first, so the engine refuses one only for its compiled size.
fn exceeds_size(error: &jsonschema::ValidationError<'_>) -> bool {
    use jsonschema::error::ValidationErrorKind;
    let mut kind = error.kind();
    for _ in 0..16 {
        match kind {
            ValidationErrorKind::Format { format } if format == "regex" => return true,
            ValidationErrorKind::PropertyNames { error } => kind = error.kind(),
            _ => break,
        }
    }
    false
}
// Share the contract's pattern memory among the distinct `patternProperties`
// name forms of the bundled `document` and its `pattern` matchers, because a
// name does not fit its first allowance, and build the validator. A name that
// does not fit what it is given refuses preparation in bundle order. Original
// locations remain with Plan, not the private constructor. The library takes one size
// setting for every name: when the names first share it, its build is their
// compilation, and otherwise it builds again within the largest compiled size
// any name was given, so no name builds more than its own.
fn share_names(
    options: jsonschema::ValidationOptions<'_, Arc<dyn jsonschema::Retrieve>, FlatJson>,
    first: pattern::Allowance,
    document: &Value,
    patterns: &operands::Patterns,
    control: &dyn WorkControl,
) -> Result<jsonschema::Validator<FlatJson>, NoVerdict> {
    control.check()?;
    let Some(Value::Object(definitions)) = document.get("$defs") else {
        return Err(program_failure());
    };
    let mut seen = BTreeSet::new();
    let mut forms = Vec::new();
    for schema in definitions.values() {
        if let Some(Value::Object(names)) = schema.get("patternProperties") {
            for form in names.keys() {
                if seen.insert(form.as_str()) {
                    forms.push(form.as_str());
                }
            }
        }
    }
    let mut options = Some(options);
    let mut built = None;
    let mut together = |allowance| {
        if control.check().is_err() {
            return false;
        }
        let shared = options
            .take()
            .expect("the names are compiled together once")
            .with_pattern_options(pattern_options(allowance));
        built = Some((allowance, shared.build(document)));
        options = Some(shared);
        if control.check().is_err() {
            return false;
        }
        built.as_ref().is_some_and(|(_, built)| built.is_ok())
    };
    let mut largest = 0;
    for given in patterns.share_with(&forms, &mut together, control) {
        match given {
            Ok(compiled) => largest = largest.max(compiled),
            Err(unsupported) => {
                let detail = unsupported_pattern(unsupported, "patternProperties");
                return Err(detail);
            }
        }
    }
    control.check()?;
    let options = options.expect("the options are returned");
    let allowance = pattern::Allowance {
        compiled: largest,
        cache: first.cache,
    };
    // The first round's build is the validator when it gave every name what
    // it is given; otherwise it is dropped before the library builds again.
    match built {
        Some((built, Ok(validator))) if built == allowance => return Ok(validator),
        other => drop(other),
    }
    control.check()?;
    let result = options
        .with_pattern_options(pattern_options(allowance))
        .build(document)
        .map_err(|error| preparation_error(&error));
    control.check()?;
    result
}
fn pattern_options(allowance: pattern::Allowance) -> jsonschema::PatternOptions<jsonschema::Regex> {
    jsonschema::PatternOptions::regex()
        .size_limit(allowance.compiled)
        .dfa_size_limit(allowance.cache)
}
fn preparation_error(error: &jsonschema::ValidationError<'_>) -> NoVerdict {
    if exceeds_size(error) {
        return unsupported_pattern(pattern::Unsupported::Size, "patternProperties");
    }
    no_verdict(
        NoVerdictReason::ConservativePreparation,
        "evaluator-preparation",
        "the evaluator could not prepare the projected schema",
    )
}
struct Compiled {
    validator: jsonschema::Validator<FlatJson>,
    deepest: Option<usize>,
}
impl Compiled {
    fn nests_within(&self, value: &JsonValue, deepest: Option<usize>) -> bool {
        match deepest {
            None => false,
            Some(usize::MAX) => true,
            Some(limit) => {
                backend::node_count(value.view()) <= limit || backend::depth_at_most(value, limit)
            }
        }
    }
    fn pass<T>(&self, evaluate: impl FnOnce() -> T) -> Result<T, NoVerdict> {
        let result = backend::string_scope(|| pass::scope(evaluate));
        // An exact predicate may return provisional false after a shared JSON
        // work charge stops. Preserve that observed cause before a Boolean or
        // a less specific private predicate stop can leave this boundary.
        if let Some(detail) = inherited_stop() {
            return Err(detail);
        }
        let (result, stop) = result.map_err(|_| {
            no_verdict(
                NoVerdictReason::UnsupportedCapability,
                "lone-surrogate-instance",
                "a reached keyword requires Unicode scalar string interpretation",
            )
        })?;
        match stop {
            None => Ok(result),
            Some(pass::Stop::Arithmetic) => Err(no_verdict(
                NoVerdictReason::LimitExceeded,
                "numeric-arithmetic-limit",
                "numeric arithmetic exceeds the admitted evaluation limit",
            )),
            Some(pass::Stop::Unmatched(reason)) => Err(unsupported_pattern(reason, "pattern")),
        }
    }
}
#[cfg(test)]
mod construction_boundary_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn observed_regex_cancellation_is_latched_before_any_retry() {
        struct Once(AtomicUsize);
        impl WorkControl for Once {
            fn is_cancelled(&self) -> bool {
                self.0.fetch_add(1, Ordering::Relaxed) == 1
            }
        }
        openbindings_internal_schema::preparation::Attempt::run(1000, 1000, &|| false, |attempt| {
            let caller = Once(AtomicUsize::new(0));
            let bridge = ConstructionControl {
                caller: &caller,
                attempt,
                cancelled: std::sync::atomic::AtomicBool::new(false),
            };
            entries::take();
            assert!(
                pattern::checked_compile(
                    "a|b",
                    pattern::Allowance {
                        compiled: pattern::FIRST,
                        cache: 0
                    },
                    &bridge
                )
                .is_none()
            );
            assert!(
                pattern::checked_compile(
                    "a|b",
                    pattern::Allowance {
                        compiled: pattern::FIRST,
                        cache: 0
                    },
                    &bridge
                )
                .is_none()
            );
            assert_eq!(bridge.check(), Err(Cancelled));
        })
        .unwrap();
    }
}
