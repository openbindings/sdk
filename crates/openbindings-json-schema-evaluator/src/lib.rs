#![doc = include_str!("../README.md")]
// A method that returns `Self` is a builder step; dropping its result loses the change.
#![warn(clippy::return_self_not_must_use)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
mod bundle;
#[cfg(test)]
extern crate self as openbindings_json_schema_evaluator;
mod depth;
mod es2020_properties;
mod explain;
mod missing;
mod operands;
mod pass;
mod pattern;
mod unevaluated_walk;
use openbindings::*;
use openbindings_internal_json::backend::{self, FlatJson};
use openbindings_internal_schema::{Program as EvaluationProgram, SchemaSpace, SuppliedSet};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
/// Default-evaluator limits. A limit that protects the process (its stack or
/// its memory) has a default; one that would only bound time is the
/// application's to set, and is off unless it does. Exceeding a limit yields
/// no verdict.
pub struct Limits {
    /// Evaluation work units per verdict or explanation pass. `None`, the
    /// default, sets no cap: the time a pass takes then grows with the schema,
    /// exponentially for some (nested `anyOf` holders of
    /// `unevaluatedProperties`), and with the value, so an application that
    /// evaluates untrusted contracts or values within a deadline sets one. `Some(n)` charges one unit for each
    /// schema a reference enters and for each element, member and literal
    /// comparison step, and gives `evaluation-work-limit` once a pass spends
    /// `n`; preparing with a cap compiles a slightly larger validator.
    pub evaluation_steps: Option<usize>,
    /// The deepest nesting of schema evaluations a value may need; default
    /// 1024. Evaluation recurses once for each schema it enters, so this
    /// bounds its stack use. Before evaluating a value, the evaluator bounds
    /// that nesting from the contract's schemas and the value's depth; a value
    /// whose bound exceeds this limit yields `evaluation-depth-limit` without
    /// being evaluated. A contract holding `unevaluatedProperties` or
    /// `unevaluatedItems`, whose library validators recurse in ways the bound
    /// does not model, is also guarded as it evaluates: a pass that uses more
    /// than 128 bytes of stack per unit of this limit (128 KiB at the default)
    /// stops with `evaluation-depth-limit`. At the default, release builds
    /// evaluate the recursive contract shapes the evaluator's stack test covers
    /// on a 256 KiB thread stack.
    pub evaluation_depth: usize,
    /// Maximum projected JSON nesting admitted to evaluator compilation; default 512.
    pub compile_json_depth: usize,
    /// Maximum UTF-8 bytes in each schema regular expression, and in the form
    /// the linear-time engine compiles it from; default 1 MiB (1,048,576
    /// bytes). A pattern whose form would be longer gives `pattern-size-limit`.
    pub pattern_bytes: usize,
    /// Maximum group nesting in a schema regular expression; default 32. The
    /// regular-expression compiler recurses at each group level.
    pub pattern_depth: usize,
    /// Maximum UTF-8 bytes in the forms the evaluator keeps for one contract's
    /// schema regular expressions: each distinct form of its `pattern` values
    /// and the form of each `patternProperties` name; default 4 MiB (4,194,304
    /// bytes). The forms spell every set out, so they can be far longer than
    /// the patterns (`\S` takes 191 bytes). Preparation counts each form
    /// as it is written and refuses with `pattern-total-byte-limit` at the
    /// first that passes the limit.
    pub total_pattern_bytes: usize,
    /// Schema resources one contract may reach, counted as its projection
    /// holds them: the document resource holding the contract's schema and
    /// each embedded `$id` resource, supplied resource and packaged
    /// meta-schema resource it reaches. Ordinary contracts reach one or a
    /// few. `None`, the default, sets no cap. `Some(n)` refuses a contract
    /// reaching more than `n` with `LimitExceeded` and `schema-resource-limit`
    /// before compiling it, so an application preparing untrusted documents or
    /// resource sets can bound preparation at the size it chooses.
    pub schema_resources: Option<usize>,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            evaluation_steps: None,
            evaluation_depth: 1024,
            compile_json_depth: 512,
            pattern_bytes: 1024 * 1024,
            pattern_depth: 32,
            total_pattern_bytes: 4 * 1024 * 1024,
            schema_resources: None,
        }
    }
}
// The most definitions dynamic specialization may produce for one contract:
// as many as the projection admits schema nodes.
const SPECIALIZED_DEFINITIONS: usize = 100_000;
// The most JSON dynamic specialization may add to the projected program in
// specialized copies, counted on the SDK's own output as one unit for each
// JSON value and one for each byte of its strings and member names, and the
// most dynamic-scope entries its distinct contexts may hold together.
const SPECIALIZED_OUTPUT: usize = 16 * 1024 * 1024;
const CONTEXT_ENTRIES: usize = 4_000_000;
// The stack a guarded pass may use for each unit of `evaluation_depth`:
// 128 KiB at the default, which leaves a 256 KiB thread room for the frames
// one level of the value adds before the next check.
const STACK_PER_DEPTH: usize = 128;
// The heaviest annotation walk an unevaluated keyword may follow, weighed by
// what each reference adds (`bundle::REFERENCE_WALK` and the rest): 26 `$ref`s,
// 8 levels of `allOf: [{"$ref": ...}]` inheritance, or 7 through `then` or
// `dependentSchemas`. The library compiles the walk by recursion; release
// builds on Linux, macOS and Windows prepare the longest admitted walk of each
// kind, below enclosing schemas, with a fifth of a 256 KiB thread already used
// (`tests/walk_stack.rs`).
const UNEVALUATED_WALK: u32 = 105;

#[derive(Clone, Debug, Default)]
/// Optional JSON Schema 2020-12 companion with exact numeric semantics and
/// explicit resources only, composed over the unmodified `jsonschema` library.
/// It performs no network or filesystem acquisition. `format` is an
/// annotation. Patterns run on a linear-time engine, so one that needs a
/// backtracking engine (lookaround or backreferences), has a Unicode property
/// escape or holds the non-word boundary `\B` gives no verdict. A contract that reaches a resource that
/// was not supplied prepares with an empty stand-in for it, and a pass that
/// reaches the stand-in gives no verdict naming the resource. A contract in
/// which an `unevaluatedProperties` or `unevaluatedItems` keyword's
/// annotation sources reach a stand-in, a reference to a fragment of a missing
/// resource, or one that identifies no schema refuses preparation. Use a custom
/// [`SchemaEvaluator`] when different qualified capabilities are required.
///
/// A cheap check ([`Detail::Outcome`]) decides with the library's validity test
/// and reports a failure with a fixed message and no problems. An explanation
/// ([`Detail::Explain`]) reports the library's first error at the keyword's
/// original schema location, with a redacted message.
pub struct DefaultEvaluator {
    limits: Limits,
}
impl DefaultEvaluator {
    /// Construct the companion with [`Limits::default`]; no schemas are compiled or resources acquired yet.
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
fn no_verdict(reason: NoVerdictReason, code: &str, message: impl Into<String>) -> NoVerdict {
    NoVerdict::new(reason, code, message)
}
impl SchemaEvaluator for DefaultEvaluator {
    fn prepare(
        &self,
        request: &SchemaRequest,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        control.check()?;
        let (mut space, mut entry) = space(request, Vec::new())?;
        let cancellation = || control.is_cancelled();
        let mut missing = Vec::new();
        let unmapped = match space.program_unmapped(entry, &cancellation) {
            Err(failure)
                if failure.reason
                    == openbindings_internal_schema::FailureReason::ResourceUnavailable =>
            {
                // Stand in for every resource the references name but the
                // request does not supply, and project again; a contract the
                // stand-ins cannot complete refuses as it first did.
                missing = missing::uris(&space);
                if missing.is_empty() {
                    return Err(projection_failure(failure));
                }
                (space, entry) = self::space(request, missing::stand_ins(&missing).collect())?;
                space
                    .program_unmapped(entry, &cancellation)
                    .map_err(|_| projection_failure(failure))?
            }
            other => other.map_err(projection_failure)?,
        };
        Ok(Arc::new(self.compile(
            unmapped.program,
            unmapped.literals,
            &unmapped.definitions,
            &space,
            missing,
            control,
        )?))
    }
}

// The request's schema space and its entry node. The set admitted its resources
// and built their identity index once; the space shares that index and drops
// its handle when preparation returns. Stand-ins are admitted with the set's
// resources, so their space indexes those again.
fn space(
    request: &SchemaRequest,
    stand_ins: Vec<openbindings_internal_schema::ProjectedResource>,
) -> Result<(SchemaSpace, usize), NoVerdict> {
    let supplied = request.supplied_resources().supplied();
    let supplied = if stand_ins.is_empty() {
        supplied.clone()
    } else {
        openbindings_internal_schema::Supplied::new(supplied.iter().cloned().chain(stand_ins))
            .map_err(|_| program_failure())?
    };
    let space = SchemaSpace::new(
        openbindings_internal_schema::RawDocument::new(
            request.document().as_document().value().clone(),
        ),
        supplied,
    );
    let entry = space
        .document_node(request.entry())
        .expect("validated entry is a schema");
    Ok((space, entry))
}
#[cfg(test)]
#[path = "../tests/private/resource_identity.rs"]
mod resource_identity;
#[cfg(test)]
#[path = "../tests/private/supplied_reuse.rs"]
mod supplied_reuse;

impl DefaultEvaluator {
    // `literals` are the program's separated `const` and `enum` values, in an
    // arena holding only them, which the program names by index; `nodes` are
    // each projected resource's definitions' nodes in `space`, in `$defs` order.
    fn compile(
        &self,
        program: EvaluationProgram,
        literals: Vec<JsonValue>,
        nodes: &[Vec<usize>],
        space: &SchemaSpace,
        missing: Vec<String>,
        control: &dyn WorkControl,
    ) -> Result<Compiled, NoVerdict> {
        if self
            .limits
            .schema_resources
            .is_some_and(|cap| program.resources.len() > cap)
        {
            return Err(no_verdict(
                NoVerdictReason::LimitExceeded,
                "schema-resource-limit",
                "the contract reaches more schema resources than the configured limit",
            ));
        }
        let mut numbers = HashMap::new();
        let mut operands = operands::Operands::default();
        let mut resources = Vec::with_capacity(program.resources.len());
        let mut entry = None;
        for (index, resource) in program.resources.iter().enumerate() {
            control.check()?;
            let mut root =
                operands.resource(&resource.document, &literals, &self.limits, control)?;
            let Some(Value::Object(mut definitions)) =
                root.as_object_mut().and_then(|r| r.remove("$defs"))
            else {
                return Err(program_failure());
            };
            // Every schema projected from a stand-in is the missing keyword.
            if !missing.is_empty() {
                for ((_, schema), &node) in definitions.iter_mut().zip(&nodes[index]) {
                    if let Some(at) = space
                        .source_uri(node)
                        .and_then(|uri| missing.binary_search_by(|m| m.as_str().cmp(uri)).ok())
                    {
                        *schema = serde_json::json!({ missing::KEYWORD: at });
                    }
                }
            }
            for (name, &node) in definitions.keys().zip(&nodes[index]) {
                let number = name
                    .strip_prefix('n')
                    .and_then(|n| n.parse::<u32>().ok())
                    .ok_or_else(program_failure)?;
                numbers.insert(number, node);
            }
            if resource.uri == program.entry_uri {
                entry = root.get("$ref").and_then(Value::as_str).map(str::to_owned);
            }
            resources.push(bundle::Resource {
                uri: resource.uri.clone(),
                definitions,
            });
        }
        let entry = entry.ok_or_else(program_failure)?;
        // Read every pattern while the schemas still have their original
        // locations, counting the forms kept for them together.
        let mut kept = Kept::default();
        for resource in &mut resources {
            for (name, schema) in resource.definitions.iter_mut() {
                control.check()?;
                let number = name[1..].parse::<u32>().map_err(|_| program_failure())?;
                patterns(schema, &self.limits, &mut operands, &mut kept).map_err(
                    |(keyword, mut detail)| {
                        detail.location = numbers
                            .get(&number)
                            .map(|&node| keyword_location(space, node, keyword));
                        detail
                    },
                )?;
            }
        }
        drop(kept);
        // The library's unevaluated validators recurse in ways the static
        // bound does not model, so a contract holding one is guarded: every
        // read of a child instance checks the stack its pass has used, once
        // for every level of the value.
        let guarded = resources.iter().any(|resource| {
            resource.definitions.values().any(|schema| {
                schema.get("unevaluatedProperties").is_some()
                    || schema.get("unevaluatedItems").is_some()
            })
        });
        let mut bundle = bundle::bundle(
            resources,
            &entry,
            &bundle::Options {
                max_definitions: SPECIALIZED_DEFINITIONS,
                max_size: SPECIALIZED_OUTPUT,
                max_context_entries: CONTEXT_ENTRIES,
                markers: self.limits.evaluation_steps.is_some(),
            },
        )
        .map_err(|_| {
            no_verdict(
                NoVerdictReason::LimitExceeded,
                "program-specialization-limit",
                "the contract's dynamic references would specialize too many schemas",
            )
        })?;
        if let Some(at) = missing::behind_unevaluated(&bundle) {
            let mut detail = no_verdict(
                NoVerdictReason::ResourceUnavailable,
                "resource-unavailable",
                "static preparation requires a resource that was not supplied",
            );
            detail.location = missing.get(at as usize).map(|uri| {
                SchemaLocation::new(
                    Some(uri.clone()),
                    Pointer::from_utf16(&[]).expect("the root pointer"),
                )
            });
            return Err(detail);
        }
        unevaluated_walk::apply(&mut bundle);
        let stack = guarded.then(|| self.limits.evaluation_depth.saturating_mul(STACK_PER_DEPTH));
        let bound =
            depth::bound(&bundle.definitions, bundle.entry, bundle.entry_weight).map_err(|_| {
                no_verdict(
                    NoVerdictReason::ConservativePreparation,
                    "evaluation-cycle",
                    "evaluation could recurse without consuming the instance",
                )
            })?;
        control.check()?;
        let locations = explain::Locations::new(space, &numbers, &bundle);
        let holders: Vec<bool> = bundle
            .schemas
            .iter()
            .map(|schema| {
                schema.get("unevaluatedProperties").is_some()
                    || schema.get("unevaluatedItems").is_some()
            })
            .collect();
        if let Some((length, holder)) = depth::longest_walk(&bundle.definitions, &holders)
            && length > UNEVALUATED_WALK
        {
            let mut detail = no_verdict(
                NoVerdictReason::LimitExceeded,
                "unevaluated-walk-limit",
                "an unevaluated keyword's annotation sources nest more deeply than the evaluator admits",
            );
            // The holder's keyword whose walk it is, at its original location.
            let schema = &bundle.schemas[holder as usize];
            let keyword = if schema.get("unevaluatedProperties").is_some() {
                "unevaluatedProperties"
            } else {
                "unevaluatedItems"
            };
            detail.location = locations
                .locate(&format!(
                    "{}#/$defs/{}/{keyword}",
                    bundle::URI,
                    bundle.names[holder as usize]
                ))
                .map(|(location, _)| location);
            return Err(detail);
        }
        let document = bundle.document();
        let validator = operands
            .apply(jsonschema::options_for::<FlatJson>())
            .with_draft(jsonschema::Draft::Draft202012)
            .with_retriever(NoRetrieval)
            .should_validate_formats(false)
            .with_pattern_options(
                jsonschema::PatternOptions::regex().size_limit(pattern::COMPILED_SIZE),
            )
            .build(&document)
            .map_err(|error| preparation_error(&error, &locations))?;
        control.check()?;
        Ok(Compiled {
            validator,
            locations,
            deepest: bound.deepest(self.limits.evaluation_depth as u64),
            deepest_explained: bound.deepest(self.limits.evaluation_depth as u64 / 2),
            stack,
            limits: self.limits.clone(),
            missing,
        })
    }
}
fn program_failure() -> NoVerdict {
    no_verdict(
        NoVerdictReason::EvaluatorFailure,
        "program-shape",
        "the projected program has an unexpected shape",
    )
}
fn keyword_location(space: &SchemaSpace, node: usize, keyword: &str) -> SchemaLocation {
    let mut at = space.location(node);
    backend::push_pointer(&mut at.pointer, &format!("/{keyword}"));
    original_location(at)
}

// Read each `pattern` and `patternProperties` name of one schema, or refuse
// it, naming the keyword. The SDK's `pattern` keyword matches a pattern
// through its reading, and answers no verdict when a string reaches one the
// evaluator does not match. The library matches `patternProperties` names
// itself, so each is written in its linear form, and one without such a form
// refuses the contract.
fn patterns(
    schema: &mut Value,
    limits: &Limits,
    operands: &mut operands::Operands,
    kept: &mut Kept,
) -> Result<(), (&'static str, NoVerdict)> {
    let Value::Object(schema) = schema else {
        return Ok(());
    };
    // Count each form the evaluator keeps as it is written, so preparation
    // stops at the first that passes the total.
    let mut count = |keyword: &'static str, form: &str| {
        kept.bytes = kept.bytes.saturating_add(form.len());
        if kept.bytes > limits.total_pattern_bytes {
            Err((
                keyword,
                no_verdict(
                    NoVerdictReason::LimitExceeded,
                    "pattern-total-byte-limit",
                    "the forms of the contract's patterns exceed the admitted total byte limit",
                ),
            ))
        } else {
            Ok(())
        }
    };
    if let Some(Value::String(source)) = schema.get("pattern") {
        let key = match kept.read.get(source) {
            Some(key) => key.clone(),
            None => {
                let reading = read_pattern(source, "pattern", limits)
                    .map_err(|detail| ("pattern", detail))?;
                // Patterns with the same form share one matcher, which keeps
                // the form.
                if let pattern::Reading::Linear(linear) = &reading
                    && !operands.has_pattern(&linear.form)
                {
                    count("pattern", &linear.form)?;
                }
                operands.pattern(reading)
            }
        };
        if let Some(Value::String(source)) =
            schema.insert("pattern".into(), Value::String(key.clone()))
        {
            kept.read.entry(source).or_insert(key);
        }
    }
    // The library matches these names itself, so the bundle keeps each form.
    if let Some(Value::Object(names)) = schema.get_mut("patternProperties") {
        let original = std::mem::take(names);
        for (source, subschema) in original {
            let refuse = |detail| ("patternProperties", detail);
            let mut form =
                match read_pattern(&source, "patternProperties", limits).map_err(refuse)? {
                    pattern::Reading::Linear(linear) => linear.form,
                    pattern::Reading::Unsupported(unsupported) => {
                        return Err(refuse(unsupported_pattern(unsupported)));
                    }
                };
            // Two names may share a form; an empty group keeps them apart.
            while names.contains_key(&form) {
                form.push_str("(?:)");
            }
            count("patternProperties", &form)?;
            form.shrink_to_fit();
            names.insert(form, subschema);
        }
    }
    Ok(())
}
// What preparation has kept for a contract's patterns so far.
#[derive(Default)]
struct Kept {
    // The bytes of the forms kept.
    bytes: usize,
    // The value written for each `pattern` already read, so a repeated
    // pattern is read once.
    read: HashMap<String, String>,
}
fn read_pattern(
    source: &str,
    keyword: &str,
    limits: &Limits,
) -> Result<pattern::Reading, NoVerdict> {
    if source.len() > limits.pattern_bytes {
        return Err(no_verdict(
            NoVerdictReason::LimitExceeded,
            "pattern-byte-limit",
            "pattern exceeds the admitted byte limit",
        ));
    }
    let reading =
        pattern::read(source, limits.pattern_depth, limits.pattern_bytes).map_err(|refusal| {
            match refusal {
                pattern::Refusal::Depth => no_verdict(
                    NoVerdictReason::LimitExceeded,
                    "pattern-depth-limit",
                    "pattern nesting exceeds the admitted limit",
                ),
                pattern::Refusal::Invalid => no_verdict(
                    NoVerdictReason::ConservativePreparation,
                    "schema-pattern-compilation",
                    format!("the schema {keyword} regular expression could not be compiled"),
                ),
            }
        })?;
    // A form longer than the pattern byte limit is neither written out nor
    // compiled: its parse alone could take memory far beyond the engine's
    // compiled size limit.
    Ok(reading)
}
fn unsupported_pattern(unsupported: pattern::Unsupported) -> NoVerdict {
    match unsupported {
        pattern::Unsupported::Backtracking => no_verdict(
            NoVerdictReason::UnsupportedCapability,
            "pattern-backtracking",
            "the schema pattern needs a backtracking engine, which this evaluator does not run",
        ),
        pattern::Unsupported::PropertyEscape => no_verdict(
            NoVerdictReason::UnsupportedCapability,
            "unicode-property-matching",
            "Unicode property-escape matching is outside the qualified evaluator capability",
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
fn preparation_error(
    error: &jsonschema::ValidationError<'_>,
    locations: &explain::Locations,
) -> NoVerdict {
    use jsonschema::error::ValidationErrorKind;
    // Read only trusted kind metadata, never dependency Display text or schema
    // values. Every pattern was read first, so the engine refuses one only
    // for its compiled size.
    let mut kind = error.kind();
    for _ in 0..16 {
        match kind {
            ValidationErrorKind::Format { format } if format == "regex" => {
                let mut detail = no_verdict(
                    NoVerdictReason::LimitExceeded,
                    "pattern-size-limit",
                    "a schema regular expression exceeds the engine's size limits",
                );
                detail.location = locations
                    .locate(&format!("{}#{}", bundle::URI, error.schema_path().as_str()))
                    .map(|(location, _)| location);
                return detail;
            }
            ValidationErrorKind::PropertyNames { error } => kind = error.kind(),
            _ => break,
        }
    }
    no_verdict(
        NoVerdictReason::ConservativePreparation,
        "evaluator-preparation",
        "the evaluator could not prepare the projected schema",
    )
}

struct Compiled {
    validator: jsonschema::Validator<FlatJson>,
    locations: explain::Locations,
    // The deepest value the depth bound admits within the limit, and within
    // half of it for explanations; `usize::MAX` when every value is.
    deepest: Option<usize>,
    deepest_explained: Option<usize>,
    // The stack a guarded contract's pass may use, in bytes.
    stack: Option<usize>,
    limits: Limits,
    // The resources the contract reaches but the request did not supply, by
    // the index their stand-ins' keyword holds.
    missing: Vec<String>,
}
fn verdict_only_failure() -> ValueOutcome {
    ValueOutcome::Fails {
        message: "value does not satisfy the contract".into(),
        problems: Vec::new(),
        problems_complete: false,
    }
}
impl PreparedSchema for Compiled {
    fn validate(
        &self,
        value: &JsonValue,
        detail: Detail,
        control: &dyn WorkControl,
    ) -> ValueOutcome {
        if let Err(detail) = control.check() {
            let detail = detail.into();
            return ValueOutcome::NoVerdict { detail };
        }
        if !self.nests_within(value, self.deepest) {
            return ValueOutcome::NoVerdict {
                detail: no_verdict(
                    NoVerdictReason::LimitExceeded,
                    "evaluation-depth-limit",
                    "evaluation depth limit reached",
                ),
            };
        }
        let valid = self.pass(|| Ok(self.validator.is_valid(backend::view(value))));
        // A completed verdict pass establishes its verdict: cancellation
        // observed later never discards it.
        match valid {
            Err(detail) => ValueOutcome::NoVerdict { detail },
            Ok(true) => ValueOutcome::Satisfies,
            Ok(false) if detail == Detail::Outcome => verdict_only_failure(),
            // An established failure survives interruption of optional diagnostics.
            Ok(false) => self.diagnostics(value, control),
        }
    }
}
impl Compiled {
    // Whether evaluating `value` nests no more than `limit` schemas deep. The
    // bound grows with the value's depth only for a recursive contract; the
    // value's node count bounds its depth without reading it, and otherwise
    // the scan stops at the first container nested too deep.
    fn nests_within(&self, value: &JsonValue, deepest: Option<usize>) -> bool {
        match deepest {
            None => false,
            Some(usize::MAX) => true,
            Some(deepest) => {
                backend::node_count(value.view()) <= deepest
                    || backend::depth_at_most(value, deepest)
            }
        }
    }
    // One verdict or diagnostic pass.
    fn pass<T>(&self, evaluate: impl FnOnce() -> Result<T, NoVerdict>) -> Result<T, NoVerdict> {
        let run = || pass::scope(evaluate);
        let allowed = || match self.limits.evaluation_steps {
            Some(steps) => openbindings_internal_json::work::scope(steps, run),
            None => Ok(run()),
        };
        // A guarded pass that used more stack than it may stopped descending,
        // so its result answers nothing.
        let (evaluated, deep) = match self.stack {
            None => (backend::string_scope(allowed), false),
            Some(bytes) => {
                match backend::string_scope(|| {
                    openbindings_internal_json::work::guard(bytes, allowed)
                }) {
                    Ok((evaluated, deep)) => (Ok(evaluated), deep),
                    Err(lone) => (Err(lone), false),
                }
            }
        };
        let evaluated = evaluated.map_err(|_| {
            no_verdict(
                NoVerdictReason::UnsupportedCapability,
                "lone-surrogate-instance",
                "a reached keyword requires Unicode scalar string interpretation",
            )
        })?;
        if deep {
            return Err(no_verdict(
                NoVerdictReason::LimitExceeded,
                "evaluation-depth-limit",
                "evaluation depth limit reached",
            ));
        }
        match evaluated {
            Err(openbindings_internal_json::work::Spent) => Err(no_verdict(
                NoVerdictReason::LimitExceeded,
                "evaluation-work-limit",
                "evaluation work limit reached",
            )),
            Ok((_, Some(pass::Stop::Arithmetic))) => Err(no_verdict(
                NoVerdictReason::LimitExceeded,
                "numeric-arithmetic-limit",
                "numeric arithmetic exceeds the admitted evaluation limit",
            )),
            Ok((_, Some(pass::Stop::Unmatched(unsupported)))) => {
                Err(unsupported_pattern(unsupported))
            }
            Ok((_, Some(pass::Stop::Missing(at)))) => {
                let mut detail = no_verdict(
                    NoVerdictReason::ResourceUnavailable,
                    "resource-unavailable",
                    "evaluation reached a resource that was not supplied",
                );
                detail.location = self.missing.get(at as usize).map(|uri| {
                    SchemaLocation::new(
                        Some(uri.clone()),
                        Pointer::from_utf16(&[]).expect("the root pointer"),
                    )
                });
                Err(detail)
            }
            Ok((result, None)) => result,
        }
    }
    fn diagnostics(&self, value: &JsonValue, control: &dyn WorkControl) -> ValueOutcome {
        // Validity is already established. Failure to obtain detail (including
        // cancellation or exhausted work) must never turn it into no-verdict.
        // Building the library's error takes about twice the stack per level a
        // check takes, so a value is explained only within half the limit.
        let explains = self.nests_within(value, self.deepest_explained);
        let problem = (explains).then(|| {
            self.pass(|| {
                control.check()?;
                let Err(error) = self.validator.validate(backend::view(value)) else {
                    return Ok(None);
                };
                let Some((schema, keyword)) = error
                    .absolute_keyword_location()
                    .and_then(|uri| self.locations.locate(uri.as_str()))
                else {
                    return Ok(None);
                };
                control.check()?;
                let instance = instance_pointer(error.instance_path().as_str());
                let (code, message) = match keyword {
                    // The library reports the transform's own keyword; the
                    // problem names the one it stands for.
                    Some(keyword) => (keyword.to_owned(), PATTERN_MEMBER.to_owned()),
                    None => (
                        error.kind().keyword().to_owned(),
                        diagnostic_message(error.kind()),
                    ),
                };
                Ok(Some(ValueProblem::new(instance, schema, code, message)))
            })
        });
        let problems = match problem {
            Some(Ok(Some(problem))) => {
                debug_assert!(value.at_pointer(&problem.instance).is_some());
                // The library's first error is the only problem.
                vec![problem]
            }
            _ => Vec::new(),
        };
        ValueOutcome::Fails {
            message: "value does not satisfy the contract".into(),
            problems,
            // A first error never claims exhaustive diagnostics.
            problems_complete: false,
        }
    }
}

// The library's instance path is the failing node's exact pointer from the
// value. It escapes `~` and `/` as RFC 6901 does and writes indices in decimal,
// and it spells each member name as the value does: evaluation presents scalar
// names, including those holding the view's marker U+E000, as themselves, and
// a name with an unpaired unit can only be read through `AsRef<str>` or
// `Cow<str>`, which marks the pass unsupported: a path built in such a pass
// may hold the view's spelling, but the pass and its problem are discarded.
fn instance_pointer(path: &str) -> Pointer {
    backend::escaped_pointer(path.to_owned())
}

const PATTERN_MEMBER: &str =
    "an object member does not satisfy the schema of a pattern its name matches";

// A fixed message for each kind of error, never the library's Display text,
// which renders schema and instance values.
fn diagnostic_message(kind: &jsonschema::error::ValidationErrorKind) -> String {
    use jsonschema::error::ValidationErrorKind as K;
    let keyword_message = match kind.keyword() {
        "pattern" => Some("string does not match the schema pattern"),
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
        K::PropertyNames { .. } => {
            "object contains a member name that does not satisfy the propertyNames schema"
        }
        K::UniqueItems => "array contains equal items where unique items are required",
        _ => "value does not satisfy the constraint at the schema location",
    };
    message.into()
}

fn original_location(at: openbindings_internal_schema::Location) -> SchemaLocation {
    SchemaLocation::new(at.resource, at.pointer)
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
    let mut detail = NoVerdict::new(reason, f.code, f.message);
    detail.location = f.location.map(original_location);
    detail
}
