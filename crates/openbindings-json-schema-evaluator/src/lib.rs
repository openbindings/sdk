#![doc = include_str!("../README.md")]
// A method that returns `Self` is a builder step; dropping its result loses the change.
#![warn(clippy::return_self_not_must_use)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
mod bundle;
#[cfg(test)]
extern crate self as openbindings_json_schema_evaluator;
mod cycles;
mod depth;
mod es2020_properties;
mod explain;
mod missing;
mod operands;
mod pass;
mod pattern;
mod stack;
mod unevaluated_walk;
use openbindings::*;
use openbindings_internal_json::backend::{self, FlatJson};
#[cfg(feature = "conversion-check")]
use openbindings_internal_schema::Program as EvaluationProgram;
use openbindings_internal_schema::{Definitions, SchemaSpace, SuppliedSet};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
/// Default-evaluator limits. A limit that protects the process (its stack or
/// its memory) has a default. `evaluation_steps`, which bounds only time, is
/// the application's to set and is off unless it does, and so is
/// `schema_resources`, which bounds the time and memory of compiling a value contract
/// that reaches many resources. Exceeding a limit yields no verdict.
pub struct Limits {
    /// Evaluation work units per verdict or explanation pass. `None`, the
    /// default, sets no cap: the time a pass takes then grows with the schema,
    /// exponentially for some (an `anyOf` naming the next level twice), and
    /// with the value, so an application that evaluates untrusted value contracts
    /// or values within a deadline sets one. `Some(n)` charges one unit for each
    /// schema a reference enters and for each element, member and literal
    /// comparison step, and gives `evaluation-work-limit` once a pass spends
    /// `n`. Preparing with a cap places a work marker before every reference,
    /// which wraps it in one more schema, so the validator is slightly larger
    /// and each reference a value's evaluation follows nests one more schema
    /// evaluation. Within the same `stack_bytes`, a recursive value contract
    /// therefore decides fewer levels with a cap, at worst about half as many:
    /// an array nested through one subschema reference per level nests five
    /// schema evaluations per level instead of three, so at the default stack
    /// it decides 203 levels instead of 340.
    pub evaluation_steps: Option<usize>,
    /// Bytes of stack the thread that prepares value contracts and evaluates values
    /// has; default 256 KiB (262,144 bytes). Preparation and evaluation
    /// recurse with the value contract and the value, so the evaluator sizes three
    /// bounds from this stack, each with a stack cost measured in release
    /// builds on Linux, macOS and Windows:
    ///
    /// - A value is evaluated only if the schema evaluations it could nest
    ///   fit 256 bytes of this stack each (1,024 at the default). Before
    ///   evaluating a value, the evaluator bounds that nesting from the
    ///   value contract's schemas and the value's depth; a value whose bound is
    ///   larger yields `evaluation-depth-limit` without being evaluated, and
    ///   an explanation is built only within half that nesting. With
    ///   `evaluation_steps` set, each reference followed nests one more
    ///   schema evaluation, so the same stack admits fewer levels of a
    ///   recursive value contract.
    /// - A value contract holding `unevaluatedProperties` or `unevaluatedItems`,
    ///   whose library validators recurse in ways the bound does not model, is
    ///   also guarded as it evaluates: a pass that uses more than half this
    ///   stack (128 KiB at the default) stops with `evaluation-depth-limit`.
    ///   The guard bounds the stack's bytes, not a depth: how deep a value it
    ///   admits depends on the frames the code takes, so it varies with the
    ///   build and the host.
    /// - The library compiles an unevaluated keyword's annotation walk by
    ///   recursion, so preparation refuses a walk weighing more than this
    ///   stack admits with `unevaluated-walk-limit`: a fifth of the stack is
    ///   left to the caller's frames, 99 KiB to the rest of preparation, and
    ///   each unit of the walk's weight takes 1 KiB (105 units at the
    ///   default).
    ///
    /// Each refusal states its arithmetic. A thread with more stack admits
    /// deeper values and longer walks; one with less refuses them sooner.
    /// These bounds were measured with stacks from 128 KiB to 2 MiB. The
    /// other recursion preparation does is bounded by `pattern_depth`,
    /// `compile_json_depth`, core's admission limits and core's fixed schema
    /// depth: whatever these limits set, preparation refuses a reached schema
    /// nested more than 256 schemas below the schema position or supplied
    /// resource root that holds it (`schema-depth-limit`). The defaults and
    /// that depth fit the default stack; a host giving less stack lowers the
    /// configurable bounds too.
    pub stack_bytes: usize,
    /// Maximum projected JSON nesting admitted to evaluator compilation; default 512.
    pub compile_json_depth: usize,
    /// Maximum UTF-8 bytes in each schema regular expression, and in the form
    /// the linear-time engine compiles it from; default 1 MiB (1,048,576
    /// bytes). A pattern whose form would be longer gives `pattern-size-limit`.
    pub pattern_bytes: usize,
    /// Maximum group nesting in a schema regular expression; default 32. The
    /// regular-expression compiler recurses at each group level.
    pub pattern_depth: usize,
    /// Maximum UTF-8 bytes in the forms the evaluator keeps for one value contract's
    /// schema regular expressions: each distinct form of its `pattern` values
    /// and the form of each `patternProperties` name; default 4 MiB (4,194,304
    /// bytes). The forms spell every set out, so they can be far longer than
    /// the patterns (`\S` takes 191 bytes). Preparation counts each form
    /// as it is written and refuses with `pattern-total-byte-limit` at the
    /// first that passes the limit.
    pub total_pattern_bytes: usize,
    /// Bytes of memory one value contract's schema regular expressions may
    /// use together, in the SDK's run matchers and the linear-time engine;
    /// default 64 MiB (67,108,864 bytes). A distinct form of a `pattern`
    /// value that is one anchored sequence of single-character runs, every
    /// run but the last with a fixed count, is matched by a run matcher, which
    /// holds 104 bytes, 24 for each step (adjacent literal characters share
    /// one, as adjacent runs of one set do), 16 for each character of a short
    /// fixed part of ASCII it holds as a table instead, 8 for each range of a
    /// run's members past ASCII and a byte for each byte of literal text. The
    /// value contract compiles each other distinct `pattern` form, and the
    /// form of each `patternProperties` name, through the regex crate's two
    /// documented size settings. A quarter of the memory, at most 2 MiB for
    /// each compiled pattern, is their search caches (`dfa_size_limit`), in
    /// equal parts, which bound only how fast a search runs. Each form first
    /// compiles within 48 KiB (`size_limit`), and the run matchers and these
    /// first allowances share the other three quarters: at the default those
    /// are 48 MiB, which hold 1,024 first allowances, and each 48 KiB or part
    /// of it that the run matchers hold takes the place of one. Preparation
    /// reads the patterns in order and refuses with
    /// `pattern-total-memory-limit` at the first pattern past what the three
    /// quarters hold, before any is compiled, so run matchers read early can
    /// bring the refusal to an earlier compiled pattern. When the compiled
    /// patterns read by then pass the count on their own, the message states
    /// the count; otherwise it states the run matchers' bytes too. The
    /// forms that need more share what is left equally; those that need less
    /// than their part keep only what they need, to within a sixteenth, and
    /// leave the rest to the others, at most 32 MiB each. So small patterns
    /// elsewhere in a value contract change a large one's outcome only through
    /// what they hold, 48 KiB for each compiled pattern and its bytes for each
    /// run matcher, and no outcome depends on the order strings reach the
    /// patterns. The first time a form is found that does not fit its 48 KiB,
    /// the value contract's compiled patterns not yet compiled are compiled
    /// then. A `patternProperties` name that does not fit what it is given
    /// refuses preparation, and a `pattern` value, compiled the first time a
    /// string reaches it, gives that string no verdict: with
    /// `pattern-total-memory-limit`, whose message states the arithmetic, or
    /// `pattern-size-limit` when the form needs more than 32 MiB.
    pub total_pattern_memory: usize,
    /// Bytes the SDK's program for one value contract may grow by beyond its
    /// projected schemas; default 48 MiB (50,331,648 bytes). Dynamic
    /// specialization copies a schema reached through `$dynamicRef` once for
    /// each dynamic context it is reached in. Preparation estimates the memory
    /// each copy takes as the SDK holds it (160 bytes for the definition, 640
    /// for each object's map, 56 for each member, 32 for each array element
    /// and a byte for each byte of its strings and member names) and each
    /// context (64 bytes and 8 for each dynamic-scope entry), and refuses a
    /// value contract whose copies and contexts would pass the projected schemas'
    /// own estimate by more than this with `program-specialization-limit`,
    /// whose message states the arithmetic. The copies an in-place cycle's
    /// unfolding makes are estimated the same way, within what specialization
    /// leaves: past it, a cycle stands in at each of its references instead.
    /// The projected schemas themselves
    /// are bounded by core's admission limits and its 100,000 schema positions
    /// per value contract. This counts the SDK's own program, not the memory the
    /// library's compiled program takes.
    pub program_bytes: usize,
    /// Schema resources one value contract may reach, counted as its projection
    /// holds them: the document resource holding the value contract's schema and
    /// each embedded `$id` resource, supplied resource and packaged
    /// meta-schema resource it reaches. Ordinary value contracts reach one or a
    /// few. `None`, the default, sets no cap. `Some(n)` refuses a value contract
    /// reaching more than `n` with `LimitExceeded` and `schema-resource-limit`
    /// before compiling it, so an application preparing untrusted documents or
    /// resource sets can bound preparation at the size it chooses. The refusal
    /// comes after core projects the value contract, which its admission limits and
    /// 100,000 schema positions bound, and before the evaluator copies the
    /// resources and the library compiles them, so it bounds the time and
    /// memory of that compilation: an 8 MB document whose input reaches a chain
    /// of 99,990 `$id` resources prepared in 8.1 s at a 725 MB peak without a
    /// cap, and refused in 1.1 s at a 499 MB peak with a cap of 1,000.
    pub schema_resources: Option<usize>,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            evaluation_steps: None,
            stack_bytes: 256 * 1024,
            compile_json_depth: 512,
            pattern_bytes: 1024 * 1024,
            pattern_depth: 32,
            total_pattern_bytes: 4 * 1024 * 1024,
            total_pattern_memory: 64 * 1024 * 1024,
            program_bytes: 48 * 1024 * 1024,
            schema_resources: None,
        }
    }
}
#[derive(Clone, Debug, Default)]
/// Optional JSON Schema 2020-12 companion with exact numeric semantics and
/// explicit resources only, composed over the unmodified `jsonschema` library.
/// It performs no network or filesystem acquisition. `format` is an
/// annotation. Patterns run on a linear-time engine.
///
/// A value contract prepares with a stand-in for what the evaluator cannot
/// evaluate: a resource that was not supplied, a schema that could apply
/// again without consuming the instance, and a pattern, in `pattern` or as a
/// `patternProperties` name, that is invalid with Unicode semantics, needs a
/// backtracking engine (lookaround or backreferences), has a Unicode property
/// escape or holds the non-word boundary `\B`. A value whose verdict depends
/// on how a stand-in comes out gives no verdict naming it, and a value whose
/// evaluation never reaches one keeps its verdict. A value contract in
/// which an `unevaluatedProperties` or `unevaluatedItems` keyword's
/// annotation sources reach a stand-in for a missing resource, a cycle or
/// `patternProperties` names, a reference to a fragment of a missing
/// resource, or one that identifies no schema refuses preparation. Use a
/// custom [`SchemaEvaluator`] when different qualified capabilities are
/// required.
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
        let (space, entry) = space(request, Vec::new())?;
        let cancellation = || control.is_cancelled();
        let projected = match space.program_definitions(entry, &cancellation) {
            Err(failure)
                if failure.reason
                    == openbindings_internal_schema::FailureReason::ResourceUnavailable =>
            {
                // Stand in for every resource the references name but the
                // request does not supply, and project again; a contract the
                // stand-ins cannot complete refuses as it first did.
                let missing = missing::uris(&space);
                if missing.is_empty() {
                    return Err(projection_failure(failure));
                }
                let (space, entry) = self::space(request, missing::stand_ins(&missing).collect())?;
                let projected = space
                    .program_definitions(entry, &cancellation)
                    .map_err(|_| projection_failure(failure))?;
                return self.prepared(projected, &space, entry, missing, control);
            }
            other => other.map_err(projection_failure)?,
        };
        self.prepared(projected, &space, entry, Vec::new(), control)
    }
}

// The request's schema space and its entry node. The set admitted its resources
// and built their identity index once, and the revision built its schema index
// when it was validated; the space shares both and drops its handles when
// preparation returns. Stand-ins are admitted with the set's resources, so
// their space indexes those again.
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
        openbindings_internal_schema::IndexedDocument::indexed(request.document()),
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

// The projected resources converted for the library, each definition's number
// with its node in the space, the entry's projected reference, and the
// sources of the patterns preparation reads.
struct Converted<'a> {
    resources: Vec<bundle::Resource>,
    numbers: HashMap<u32, usize>,
    entry: String,
    sources: operands::Sources<'a>,
}
impl DefaultEvaluator {
    fn prepared(
        &self,
        projected: Definitions<'_>,
        space: &SchemaSpace,
        entry: usize,
        missing: Vec<String>,
        control: &dyn WorkControl,
    ) -> Result<Arc<dyn PreparedSchema>, NoVerdict> {
        if self
            .limits
            .schema_resources
            .is_some_and(|cap| projected.resources.len() > cap)
        {
            return Err(no_verdict(
                NoVerdictReason::LimitExceeded,
                "schema-resource-limit",
                "the value contract reaches more schema resources than the configured limit",
            ));
        }
        let mut operands = operands::Operands::default();
        #[cfg(feature = "conversion-check")]
        let bounds: Vec<(usize, usize)> = projected
            .resources
            .iter()
            .map(|resource| (resource.bytes, resource.nodes))
            .collect();
        let cyclic = projected.in_place_cycle;
        let converted = self.defined(projected, &mut operands, space, &missing, control)?;
        // The SDK's own tests and tools (`conversion-check`) check that the
        // definitions convert as the program's text does.
        #[cfg(feature = "conversion-check")]
        self.check_conversion(&converted, &operands, space, entry, &missing, &bounds);
        #[cfg(not(feature = "conversion-check"))]
        let _ = entry;
        Ok(Arc::new(self.compile(
            converted, operands, space, missing, cyclic, control,
        )?))
    }
    // Convert a projection's definitions as the evaluator converted the
    // program's text, which `conversion-check` still checks (`parsed`).
    fn defined<'a>(
        &self,
        projected: openbindings_internal_schema::Definitions<'a>,
        operands: &mut operands::Operands,
        space: &SchemaSpace,
        missing: &[String],
        control: &dyn WorkControl,
    ) -> Result<Converted<'a>, NoVerdict> {
        let mut numbers = HashMap::new();
        let mut sources = operands::Sources::new();
        let mut resources = Vec::with_capacity(projected.resources.len());
        let mut entry = None;
        let mut stand_ins = Vec::new();
        for resource in projected.resources {
            control.check()?;
            // Each definition's node, and the stand-ins' definitions, which
            // are the missing keyword, by name.
            for definition in &resource.definitions {
                let number = definition
                    .name
                    .strip_prefix('n')
                    .and_then(|n| n.parse::<u32>().ok())
                    .ok_or_else(program_failure)?;
                numbers.insert(number, definition.node);
                if !missing.is_empty()
                    && let Some(at) = space
                        .source_uri(definition.node)
                        .and_then(|uri| missing.binary_search_by(|m| m.as_str().cmp(uri)).ok())
                {
                    stand_ins.push((definition.name.clone(), at));
                }
            }
            let mut definitions = operands.definitions(
                resource.definitions,
                &projected.literals,
                &self.limits,
                &mut sources,
                control,
            )?;
            for (name, at) in stand_ins.drain(..) {
                if let Some(schema) = definitions.get_mut(&name) {
                    *schema = serde_json::json!({ missing::KEYWORD: at });
                }
            }
            if resource.uri == projected.entry_uri {
                entry = resource.entry;
            }
            resources.push(bundle::Resource {
                uri: resource.uri,
                definitions,
            });
        }
        Ok(Converted {
            resources,
            numbers,
            entry: entry.ok_or_else(program_failure)?,
            sources,
        })
    }
    // Convert a projection the core wrote as text: `literals` are its
    // separated `const` and `enum` values, in an arena holding only them,
    // which the program names by index; `nodes` are each projected resource's
    // definitions' nodes in `space`, in `$defs` order.
    #[cfg(feature = "conversion-check")]
    #[allow(clippy::too_many_arguments)]
    fn parsed<'a>(
        &self,
        program: &'a EvaluationProgram,
        literals: &[JsonValue],
        nodes: &[Vec<usize>],
        operands: &mut operands::Operands,
        space: &SchemaSpace,
        missing: &[String],
        control: &dyn WorkControl,
    ) -> Result<Converted<'a>, NoVerdict> {
        let mut numbers = HashMap::new();
        let mut sources = operands::Sources::new();
        let mut resources = Vec::with_capacity(program.resources.len());
        let mut entry = None;
        for (index, resource) in program.resources.iter().enumerate() {
            control.check()?;
            let mut root = operands.resource(
                &resource.document,
                literals,
                &self.limits,
                &mut sources,
                control,
            )?;
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
        Ok(Converted {
            resources,
            numbers,
            entry: entry.ok_or_else(program_failure)?,
            sources,
        })
    }
    // That the text projection, converted, gives what the definitions gave.
    #[cfg(feature = "conversion-check")]
    fn check_conversion(
        &self,
        converted: &Converted,
        operands: &operands::Operands,
        space: &SchemaSpace,
        entry: usize,
        missing: &[String],
        bounds: &[(usize, usize)],
    ) {
        let unmapped = space
            .program_unmapped(entry, &|| false)
            .expect("the text projection admits what the definitions admit");
        let mut text_operands = operands::Operands::default();
        let text = self
            .parsed(
                &unmapped.program,
                &unmapped.literals,
                &unmapped.definitions,
                &mut text_operands,
                space,
                missing,
                &std::sync::atomic::AtomicBool::new(false),
            )
            .expect("the text converts as the definitions do");
        assert_eq!(text.entry, converted.entry);
        assert_eq!(text.numbers, converted.numbers);
        assert_eq!(text.sources, converted.sources);
        assert_eq!(text.resources.len(), converted.resources.len());
        // The definitions' bounds on each resource's text hold.
        for ((resource, text), (bytes, nodes)) in unmapped
            .program
            .resources
            .iter()
            .zip(&text.resources)
            .zip(bounds)
        {
            assert_eq!(resource.uri, text.uri);
            assert!(resource.document.text().len() <= *bytes);
            assert_eq!(
                openbindings_internal_json::backend::node_count(resource.document.view()),
                *nodes
            );
        }
        for (text, converted) in text.resources.iter().zip(&converted.resources) {
            assert_eq!(text.uri, converted.uri);
            assert_eq!(text.definitions, converted.definitions);
            // Equal maps can still iterate differently, as they do when
            // serde_json keeps insertion order, so the order must agree too.
            assert_eq!(
                serde_json::to_string(&text.definitions).expect("definitions serialize"),
                serde_json::to_string(&converted.definitions).expect("definitions serialize"),
            );
        }
        assert!(text_operands.same_literals(operands));
    }
    #[allow(clippy::too_many_arguments)]
    fn compile(
        &self,
        converted: Converted,
        mut operands: operands::Operands,
        space: &SchemaSpace,
        missing: Vec<String>,
        cyclic: bool,
        control: &dyn WorkControl,
    ) -> Result<Compiled, NoVerdict> {
        let Converted {
            mut resources,
            numbers,
            entry,
            mut sources,
        } = converted;
        // Read every pattern while the schemas still have their original
        // locations, counting the forms kept for them together. A schema's
        // `patternProperties` names that the evaluator does not match are
        // left out, and a stand-in takes their place.
        let mut kept = Kept::default();
        let mut names = Vec::new();
        for resource in &mut resources {
            for (name, schema) in resource.definitions.iter_mut() {
                control.check()?;
                let number = name[1..].parse::<u32>().map_err(|_| program_failure())?;
                let source = if sources.is_empty() {
                    None
                } else {
                    sources.remove(&number)
                };
                let located = |keyword| {
                    numbers
                        .get(&number)
                        .map(|&node| keyword_location(space, node, keyword))
                };
                let unmatched = patterns(schema, source, &self.limits, &mut operands, &mut kept)
                    .map_err(|(keyword, mut detail)| {
                        detail.location = located(keyword);
                        detail
                    })?;
                if let Some(unsupported) = unmatched {
                    if let Value::Object(schema) = schema {
                        let at = Value::from(names.len());
                        // A member `additionalProperties` reaches might match
                        // one of the names, so it reaches the stand-in too.
                        if let Some(additional) = schema.remove("additionalProperties") {
                            let mut member = serde_json::Map::new();
                            member.insert(operands::NAME_MEMBER.into(), at.clone());
                            schema.insert(
                                "additionalProperties".into(),
                                serde_json::json!({ "allOf": [Value::Object(member), additional] }),
                            );
                        }
                        schema.insert(operands::NAMES.into(), at);
                    }
                    names.push(UnmatchedNames {
                        unsupported,
                        location: located("patternProperties"),
                    });
                }
            }
        }
        // Each pattern the contract compiles holds its first allowance of the
        // pattern memory, the library's `patternProperties` names and the
        // SDK's `pattern` matchers alike, and each run matcher its own bytes.
        let memory = pattern::Memory::new(
            self.limits.total_pattern_memory,
            kept.compiled,
            kept.matched,
        );
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
                program_bytes: self.limits.program_bytes,
                markers: self.limits.evaluation_steps.is_some(),
            },
        )
        .map_err(|refusal| match refusal {
            bundle::Refusal::Budget(growth) => specialization_limit(growth),
            bundle::Refusal::Shape => program_failure(),
        })?;
        // Each schema that would apply again without consuming the instance
        // is a stand-in, which names that schema's original location.
        // Its copies share what specialization leaves of the program budget.
        let budget = self.limits.program_bytes.saturating_sub(bundle.grown);
        let cycles: Vec<Option<SchemaLocation>> = if cyclic {
            let (stand_ins, _) = cycles::unfold(&mut bundle, budget);
            stand_ins
                .into_iter()
                .map(|number| {
                    numbers
                        .get(&number)
                        .map(|&node| original_location(space.location(node)))
                })
                .collect()
        } else {
            Vec::new()
        };
        // An unevaluated keyword reads its annotation sources' structure, so
        // a stand-in among them refuses preparation.
        if guarded
            && !cycles.is_empty()
            && let Some(at) = missing::behind_unevaluated(&bundle, operands::CYCLE)
        {
            let mut detail =
                in_place_cycle("static preparation found a potentially evaluated in-place cycle");
            detail.location = cycles.get(at as usize).cloned().flatten();
            return Err(detail);
        }
        if guarded
            && !names.is_empty()
            && let Some(at) = missing::behind_unevaluated(&bundle, operands::NAMES)
        {
            let names = &names[at as usize];
            let mut detail = unsupported_pattern(names.unsupported, "patternProperties");
            detail.location = names.location.clone();
            return Err(detail);
        }
        if guarded && let Some(at) = missing::behind_unevaluated(&bundle, missing::KEYWORD) {
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
        if guarded {
            unevaluated_walk::apply(&mut bundle);
        }
        let host = stack::Stack {
            bytes: self.limits.stack_bytes,
        };
        let guard = guarded.then(|| host.guard());
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
        // Without an unevaluated keyword, no definition holds a walk.
        let holders: Vec<bool> = if guarded {
            bundle
                .schemas
                .iter()
                .map(|schema| {
                    schema.get("unevaluatedProperties").is_some()
                        || schema.get("unevaluatedItems").is_some()
                })
                .collect()
        } else {
            Vec::new()
        };
        if guarded
            && let Some((length, holder)) = depth::longest_walk(&bundle.definitions, &holders)
            && length > host.walk()
        {
            let mut detail = host.walk_too_heavy(length);
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
        let standing = !missing.is_empty() || !cycles.is_empty() || !names.is_empty();
        let registered = operands::Registered {
            markers: self.limits.evaluation_steps.is_some(),
            missing: !missing.is_empty(),
            cycles: !cycles.is_empty(),
            names: !names.is_empty(),
        };
        let document = bundle.document();
        let first = memory.first();
        let keywords = operands.keywords(memory);
        let options = keywords
            .register(jsonschema::options_for::<FlatJson>(), registered)
            .with_draft(jsonschema::Draft::Draft202012)
            .with_retriever(NoRetrieval)
            .should_validate_formats(false)
            .with_pattern_options(pattern_options(first));
        // The library takes one size setting for every `patternProperties`
        // name, so the names first compile within their first allowances. When
        // one does not fit, the SDK shares the pattern memory among the names
        // and the `pattern` matchers itself, and the library compiles the
        // names again within the largest compiled size any name was given:
        // each name's form compiles to the same program within any size limit
        // it fits.
        let validator = match options.build(&document) {
            Ok(validator) => validator,
            Err(error) if exceeds_size(&error) => share_names(
                options,
                first,
                &document,
                keywords.patterns(),
                &locations,
                control,
            )?,
            Err(error) => return Err(preparation_error(&error, &locations)),
        };
        drop(document);
        control.check()?;
        let stand_ins = standing.then(|| {
            Box::new(StandIns {
                missing,
                cycles,
                names,
            })
        });
        Ok(Compiled {
            validator,
            patterns: keywords.patterns().clone(),
            locations,
            deepest: bound.deepest(host.evaluation_depth()),
            deepest_explained: bound.deepest(host.evaluation_depth() / 2),
            host,
            guard,
            limits: self.limits.clone(),
            stand_ins,
        })
    }
}
// The stand-in for one schema's `patternProperties` names the evaluator does
// not match: why the first of them is not matched, and the keyword's original
// location.
struct UnmatchedNames {
    unsupported: pattern::Unsupported,
    location: Option<SchemaLocation>,
}
fn in_place_cycle(message: &str) -> NoVerdict {
    no_verdict(
        NoVerdictReason::ConservativePreparation,
        "in-place-cycle",
        message,
    )
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

// Read each `pattern`, from its `source`, and `patternProperties` name of one
// schema, or refuse it, naming the keyword. The SDK's `pattern` keyword
// matches a pattern through its reading, and stands in when a string reaches
// one the evaluator does not match. The library matches `patternProperties`
// names itself, so each is written in its linear form. A name the evaluator
// does not match for its syntax is left out, and the first such name's reason
// is returned, so a stand-in can take their place; a name without a form for
// a limit refuses the contract.
fn patterns<'a>(
    schema: &mut Value,
    source: Option<std::borrow::Cow<'a, str>>,
    limits: &Limits,
    operands: &mut operands::Operands,
    kept: &mut Kept<'a>,
) -> Result<Option<pattern::Unsupported>, (&'static str, NoVerdict)> {
    let Value::Object(schema) = schema else {
        return Ok(None);
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
                    "the forms of the value contract's patterns exceed the admitted total byte limit",
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
            return Err((
                keyword,
                pattern_memory_limit(pattern::Shortfall::Count {
                    matched: kept.matched,
                    compiled: kept.compiled,
                    total,
                }),
            ));
        }
        Ok(())
    };
    if let Some(source) = source
        && let Some(slot) = schema.get_mut("pattern")
    {
        let key = match kept.read.get(&*source) {
            Some(key) => key.clone(),
            None => {
                let reading =
                    read_pattern(&source, limits).map_err(|detail| ("pattern", detail))?;
                // Patterns with the same form share one matcher, which keeps
                // the form and compiles it unless the run matcher matches it.
                if let pattern::Reading::Linear(linear) = &reading
                    && !operands.has_pattern(&linear.form)
                {
                    count("pattern", &linear.form, linear.simple.as_ref())?;
                }
                let key = operands.pattern(reading);
                kept.read.insert(source, key.clone());
                key
            }
        };
        *slot = Value::String(key);
    }
    // The library matches these names itself, so the bundle keeps each form.
    let mut unmatched = None;
    if let Some(Value::Object(names)) = schema.get_mut("patternProperties") {
        let original = std::mem::take(names);
        for (source, subschema) in original {
            let refuse = |detail| ("patternProperties", detail);
            let mut form = match read_pattern(&source, limits).map_err(refuse)? {
                pattern::Reading::Linear(linear) => linear.form,
                pattern::Reading::Unsupported(unsupported) if unsupported.stands_in() => {
                    unmatched.get_or_insert(unsupported);
                    continue;
                }
                pattern::Reading::Unsupported(unsupported) => {
                    return Err(refuse(unsupported_pattern(
                        unsupported,
                        "patternProperties",
                    )));
                }
            };
            // Two names may share a form; an empty group keeps them apart.
            while names.contains_key(&form) {
                form.push_str("(?:)");
            }
            count("patternProperties", &form, None)?;
            form.shrink_to_fit();
            names.insert(form, subschema);
        }
    }
    Ok(unmatched)
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
    read: HashMap<std::borrow::Cow<'a, str>, String>,
}
// A pattern's reading: an invalid pattern is read as one the evaluator does
// not match, whose match is undefined.
fn read_pattern(source: &str, limits: &Limits) -> Result<pattern::Reading, NoVerdict> {
    if source.len() > limits.pattern_bytes {
        return Err(no_verdict(
            NoVerdictReason::LimitExceeded,
            "pattern-byte-limit",
            "pattern exceeds the admitted byte limit",
        ));
    }
    // A form longer than the pattern byte limit is neither written out nor
    // compiled: its parse alone could take memory far beyond the engine's
    // compiled size limit.
    match pattern::read(source, limits.pattern_depth, limits.pattern_bytes) {
        Ok(reading) => Ok(reading),
        Err(pattern::Refusal::Invalid) => {
            Ok(pattern::Reading::Unsupported(pattern::Unsupported::Invalid))
        }
        Err(pattern::Refusal::Depth) => Err(no_verdict(
            NoVerdictReason::LimitExceeded,
            "pattern-depth-limit",
            "pattern nesting exceeds the admitted limit",
        )),
    }
}
// Why a pattern `keyword` holds is not matched.
fn unsupported_pattern(unsupported: pattern::Unsupported, keyword: &str) -> NoVerdict {
    match unsupported {
        pattern::Unsupported::Invalid => no_verdict(
            NoVerdictReason::ConservativePreparation,
            "schema-pattern-compilation",
            format!("the schema {keyword} regular expression could not be compiled"),
        ),
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
        pattern::Unsupported::Memory(shortfall) => pattern_memory_limit(shortfall),
    }
}
// The refusal states the arithmetic, so its message says why and what to raise.
fn pattern_memory_limit(shortfall: pattern::Shortfall) -> NoVerdict {
    const RAISE: &str = "raise total_pattern_memory (totalPatternMemory in TypeScript)";
    let first = bytes(pattern::FIRST);
    let message = match shortfall {
        pattern::Shortfall::Count {
            matched,
            compiled,
            total,
        } => {
            let most = pattern::Memory::most_patterns(total, 0)
                .expect("any pattern memory holds no run matcher");
            // The refusal comes at the first pattern past what the memory
            // holds for the run matchers and first allowances together. When
            // the compiled patterns read by then pass the count on their own,
            // it states the count, as it would without run matchers;
            // otherwise the run matchers' bytes are part of why it refuses.
            if matched == 0 || compiled > most {
                format!(
                    "this value contract compiles more than {most} regular expressions, as \
                     many as its {} total_pattern_memory holds at {first} each once their \
                     search caches take a quarter; {RAISE}",
                    bytes(total),
                )
            } else {
                let compiled = match compiled {
                    0 => String::new(),
                    1 => format!(" and its one compiled regular expression {first}"),
                    compiled => {
                        format!(" and its {compiled} compiled regular expressions {first} each")
                    }
                };
                format!(
                    "this value contract's run matchers hold {}{compiled}, more {}than the {} \
                     its {} total_pattern_memory holds for them once search caches take a \
                     quarter; {RAISE}",
                    bytes_up(matched),
                    if compiled.is_empty() { "" } else { "together " },
                    bytes(pattern::Memory::held(total)),
                    bytes(total),
                )
            }
        }
        pattern::Shortfall::Pattern {
            matched,
            patterns,
            total,
            cache,
            left,
            kept,
            sharing,
            compiled,
        } => {
            let holding = match (matched, patterns) {
                (0, 1) => format!(
                    "this value contract's one compiled regular expression holds {first} of \
                     its {} total_pattern_memory and {} for its search cache",
                    bytes(total),
                    bytes(cache),
                ),
                (0, _) => format!(
                    "this value contract's {patterns} compiled regular expressions hold \
                     {first} each of its {} total_pattern_memory and {} each for their search \
                     caches",
                    bytes(total),
                    bytes(cache),
                ),
                (_, 1) => format!(
                    "this value contract's run matchers hold {} of its {} total_pattern_memory, \
                     and its one compiled regular expression {first} and {} for its search cache",
                    bytes(matched),
                    bytes(total),
                    bytes(cache),
                ),
                _ => format!(
                    "this value contract's run matchers hold {} of its {} total_pattern_memory, \
                     and its {patterns} compiled regular expressions {first} each and {} each \
                     for their search caches",
                    bytes(matched),
                    bytes(total),
                    bytes(cache),
                ),
            };
            let sharing = match (sharing, kept) {
                (1, 0) => "this one, which needs more, may take it all, but".to_owned(),
                (1, kept) => format!(
                    "those that need less than an equal part of it keep {}, and this one may \
                     take the rest, but",
                    bytes(kept),
                ),
                (sharing, 0) => {
                    format!("the {sharing} that need more share it equally, but this one")
                }
                (sharing, kept) => format!(
                    "those that need less than an equal part of it keep {}, and the {sharing} \
                     that need more share the rest equally, but this one",
                    bytes(kept),
                ),
            };
            format!(
                "{holding}, which leaves {}; {sharing} does not compile within the {} that \
                 gives it; {RAISE}",
                bytes(left),
                bytes(compiled),
            )
        }
    };
    no_verdict(
        NoVerdictReason::LimitExceeded,
        "pattern-total-memory-limit",
        message,
    )
}
// The refusal states the arithmetic, so its message says why and what to raise.
fn specialization_limit(growth: bundle::Growth) -> NoVerdict {
    no_verdict(
        NoVerdictReason::LimitExceeded,
        "program-specialization-limit",
        format!(
            "this value contract's dynamic references specialize {} schemas, taking {}, and \
             {} dynamic contexts holding {} entries, taking {}, before preparation stops: \
             together they pass its projected schemas' {} by more than its {} program_bytes; \
             raise program_bytes (programBytes in TypeScript)",
            growth.definitions,
            bytes(growth.definition_bytes),
            growth.contexts,
            growth.entries,
            bytes(growth.context_bytes),
            bytes(growth.projected),
            bytes(growth.budget),
        ),
    )
}
// A count of bytes as a message states it, rounded up in the units `bytes`
// uses, so a count past a limit `bytes` states never reads as within it.
fn bytes_up(count: usize) -> String {
    let unit = if count >= 1024 * 1024 {
        1024 * 1024 / 10
    } else if count >= 1024 {
        1024
    } else {
        1
    };
    bytes(count.div_ceil(unit).saturating_mul(unit))
}
// A count of bytes as a message states it, rounded down: whole KiB below a
// MiB, MiB to a tenth above.
fn bytes(count: usize) -> String {
    const KIB: usize = 1024;
    const MIB: usize = 1024 * KIB;
    if count >= MIB {
        let tenths = count / (MIB / 10);
        match tenths % 10 {
            0 => format!("{} MiB", tenths / 10),
            tenth => format!("{}.{tenth} MiB", tenths / 10),
        }
    } else if count >= KIB {
        format!("{} KiB", count / KIB)
    } else {
        format!("{count} bytes")
    }
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
// does not fit what it is given refuses preparation at its keyword's original
// location, the first in the bundle's order. The library takes one size
// setting for every name: when the names first share it, its build is their
// compilation, and otherwise it builds again within the largest compiled size
// any name was given, so no name builds more than its own.
fn share_names(
    options: jsonschema::ValidationOptions<'_, Arc<dyn jsonschema::Retrieve>, FlatJson>,
    first: pattern::Allowance,
    document: &Value,
    patterns: &operands::Patterns,
    locations: &explain::Locations,
    control: &dyn WorkControl,
) -> Result<jsonschema::Validator<FlatJson>, NoVerdict> {
    control.check()?;
    let Some(Value::Object(definitions)) = document.get("$defs") else {
        return Err(program_failure());
    };
    let mut seen = std::collections::HashSet::new();
    let (mut forms, mut holders) = (Vec::new(), Vec::new());
    for (definition, schema) in definitions {
        if let Some(Value::Object(names)) = schema.get("patternProperties") {
            for form in names.keys() {
                if seen.insert(form.as_str()) {
                    forms.push(form.as_str());
                    holders.push(definition);
                }
            }
        }
    }
    let mut options = Some(options);
    let mut built = None;
    let mut together = |allowance| {
        let shared = options
            .take()
            .expect("the names are compiled together once")
            .with_pattern_options(pattern_options(allowance));
        built = Some((allowance, shared.build(document)));
        let fits = built.as_ref().is_some_and(|(_, built)| built.is_ok());
        options = Some(shared);
        fits
    };
    let mut largest = 0;
    for (given, definition) in patterns
        .share_with(&forms, &mut together)
        .into_iter()
        .zip(holders)
    {
        match given {
            Ok(compiled) => largest = largest.max(compiled),
            Err(unsupported) => {
                let mut detail = unsupported_pattern(unsupported, "patternProperties");
                detail.location = locations
                    .locate(&format!(
                        "{}#/$defs/{definition}/patternProperties",
                        bundle::URI
                    ))
                    .map(|(location, _)| location);
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
    options
        .with_pattern_options(pattern_options(allowance))
        .build(document)
        .map_err(|error| preparation_error(&error, locations))
}
fn pattern_options(allowance: pattern::Allowance) -> jsonschema::PatternOptions<jsonschema::Regex> {
    jsonschema::PatternOptions::regex()
        .size_limit(allowance.compiled)
        .dfa_size_limit(allowance.cache)
}
fn preparation_error(
    error: &jsonschema::ValidationError<'_>,
    locations: &explain::Locations,
) -> NoVerdict {
    // Read only trusted kind metadata, never dependency Display text or schema
    // values.
    if exceeds_size(error) {
        let mut detail = unsupported_pattern(pattern::Unsupported::Size, "patternProperties");
        detail.location = locations
            .locate(&format!("{}#{}", bundle::URI, error.schema_path().as_str()))
            .map(|(location, _)| location);
        return detail;
    }
    no_verdict(
        NoVerdictReason::ConservativePreparation,
        "evaluator-preparation",
        "the evaluator could not prepare the projected schema",
    )
}

struct Compiled {
    validator: jsonschema::Validator<FlatJson>,
    // The `pattern` keywords' matchers, which the validator shares, and their
    // sites, which name the keyword a pass stopped at.
    patterns: Arc<operands::Patterns>,
    locations: explain::Locations,
    // The deepest value the depth bound admits within the limit, and within
    // half of it for explanations; `usize::MAX` when every value is.
    deepest: Option<usize>,
    deepest_explained: Option<usize>,
    // The stack the host gives, and the stack a guarded contract's pass may
    // use, in bytes.
    host: stack::Stack,
    guard: Option<usize>,
    limits: Limits,
    // What the contract's stand-ins stand for, when it has any.
    stand_ins: Option<Box<StandIns>>,
}
struct StandIns {
    // The resources the contract reaches but the request did not supply, by
    // the index their stand-ins' keyword holds.
    missing: Vec<String>,
    // The original location of each schema that would apply again, by the
    // index its stand-in's keyword holds.
    cycles: Vec<Option<SchemaLocation>>,
    // Each schema's unmatched `patternProperties` names, likewise.
    names: Vec<UnmatchedNames>,
}
fn verdict_only_failure() -> ValueOutcome {
    ValueOutcome::Fails {
        message: "value does not satisfy the value contract".into(),
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
                detail: self.host.too_deep(self.deepest),
            };
        }
        let valid = self.pass(|| self.validator.is_valid(backend::view(value)));
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
    // One verdict or diagnostic pass, and what it evaluated unless it met a
    // reason to give no verdict. The pass carries only `evaluate`'s own
    // result, so a verdict pass moves a `bool` rather than a no-verdict.
    fn pass<T>(&self, evaluate: impl FnOnce() -> T) -> Result<T, NoVerdict> {
        let run = || pass::scope(evaluate);
        let allowed = || match self.limits.evaluation_steps {
            Some(steps) => openbindings_internal_json::work::scope(steps, run),
            None => Ok(run()),
        };
        // A guarded pass that used more stack than it may stopped descending,
        // so its result answers nothing.
        let (evaluated, deep) = match self.guard {
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
            return Err(self.host.guard_exceeded());
        }
        match evaluated {
            Err(openbindings_internal_json::work::Spent) => Err(no_verdict(
                NoVerdictReason::LimitExceeded,
                "evaluation-work-limit",
                "evaluation work limit reached",
            )),
            Ok((_, Some(stop))) => Err(self.stopped(stop)),
            Ok((result, None)) => Ok(result),
        }
    }
    // The no-verdict a pass that stopped at `stop` gives, naming where.
    fn stopped(&self, stop: pass::Stop) -> NoVerdict {
        let stand_ins = self.stand_ins.as_deref();
        match stop {
            pass::Stop::Unmatched(unsupported, site) => {
                // The `pattern` keyword that holds it, at its original location.
                let mut detail = unsupported_pattern(unsupported, "pattern");
                detail.location = self.patterns.site(site).and_then(|at| {
                    self.locations
                        .locate(&format!("{}#{}", bundle::URI, at.as_str()))
                        .map(|(location, _)| location)
                });
                detail
            }
            pass::Stop::Missing(at) => {
                let mut detail = no_verdict(
                    NoVerdictReason::ResourceUnavailable,
                    "resource-unavailable",
                    "evaluation reached a resource that was not supplied",
                );
                detail.location = stand_ins
                    .and_then(|stand_ins| stand_ins.missing.get(at as usize))
                    .map(|uri| {
                        SchemaLocation::new(
                            Some(uri.clone()),
                            Pointer::from_utf16(&[]).expect("the root pointer"),
                        )
                    });
                detail
            }
            pass::Stop::Cycle(at) => {
                let mut detail = in_place_cycle(
                    "evaluation reached a schema that would apply again without consuming the instance",
                );
                detail.location = stand_ins
                    .and_then(|stand_ins| stand_ins.cycles.get(at as usize))
                    .cloned()
                    .flatten();
                detail
            }
            pass::Stop::Names(at) => {
                let names = stand_ins.and_then(|stand_ins| stand_ins.names.get(at as usize));
                let mut detail = unsupported_pattern(
                    names.map_or(pattern::Unsupported::Invalid, |names| names.unsupported),
                    "patternProperties",
                );
                detail.location = names.and_then(|names| names.location.clone());
                detail
            }
            pass::Stop::Arithmetic => no_verdict(
                NoVerdictReason::LimitExceeded,
                "numeric-arithmetic-limit",
                "numeric arithmetic exceeds the admitted evaluation limit",
            ),
        }
    }
    // The library's first error for `value`, which the value contract fails,
    // at its keyword's original location.
    fn diagnostics(&self, value: &JsonValue, control: &dyn WorkControl) -> ValueOutcome {
        // Validity is already established. Failure to obtain detail (including
        // cancellation or exhausted work) must never turn it into no-verdict.
        // Building the library's error takes about twice the stack per level a
        // check takes, so a value is explained only within half the limit.
        let explains = self.nests_within(value, self.deepest_explained);
        let problem = (explains).then(|| {
            self.pass(|| -> Result<_, NoVerdict> {
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
            .and_then(|problem| problem)
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
            message: "value does not satisfy the value contract".into(),
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
