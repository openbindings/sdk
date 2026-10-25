# Internal dependency maintenance contract

Core and the default evaluator both use the unmodified jsonschema 0.58.6
library from the registry, under its own name. Core keeps the library's default
pattern engine (jsonschema-regex 0.58.6 and fancy-regex 0.19.2) for its fixed
checks (OBI-02 and OBI-10) and uses the library for the resource identity and
reference resolution of its reference inspection. The evaluator matches schema
patterns with the regex crate (regex 1.13.1, regex-automata 0.4.18 and
regex-syntax 0.8.11) directly. jsonschema-value 0.58.6, referencing 0.58.6 and
serde_json 1.0.151 are unmodified too. The SDK carries no fork, vendored crate
or patch of any dependency: every adaptation is SDK code, a transform of the
schemas before they reach the library, or a documented extension point.
`docs/dependency-isolation.json` records the nine upstream archive checksums
and complete Rust source digests. `tools/verify-dependency-isolation.py` checks
those sources, the single-serde graph, absence of SDK-requested arbitrary
precision, that no package is vendored or renamed, the separation below, and the
cancellation/schema boundaries. Its optional `--upstream-dir` additionally
checks the downloaded archives.

## Core on the unmodified library

Core's fixed checks compile the pinned document schema and the localized
2020-12 meta-schemas with `jsonschema::options_for::<FlatJson>()`, exact
comparisons registered through `with_keyword`, format assertion off, only local
resources, and the library's default `PatternOptions`. They have no work budget.
`pattern` is registered through `with_keyword` too, as a keyword that consults
one shared validator of `{"pattern": ...}`, compiled by the library with its
defaults, for each distinct pattern. The location plan compiles a validator for
every location of the derived schema, and each would otherwise compile its own
copy of every pattern beneath it: the name grammar about ninety times, a few
kilobytes of compiled matcher each.
Every fixed pattern is regular: the SemVer `openbindings` pattern, the name
grammar, `^x-`, `^[^#]*#?$` for `$id` and the anchor grammar. jsonschema-regex
leaves each unchanged (`^x-` takes its literal prefix path), and fancy-regex hands
a pattern without lookaround or backreference whole to the regex crate's
finite-automaton engine, whose time is linear in the string and whose memory is
bounded by the pattern. The derived document schema has no recursive reference
and the meta-schema check cuts its recursive edges, so a node meets each schema
position at most once (80 and 129 positions; tests count them), and location
guidance revalidates only beneath failing positions, a bounded number of times
for each level of the derived schema's finite depth. The admission limits
therefore bound each check's work; its memory is the stored document, a bounded
matcher cache and the capped findings; and its stack depth follows the fixed
schemas, not the instance (`document_stack` runs 12,000-deep schemas on a
256 KiB thread). The library catches the regex crate's known search panic
(rust-lang/regex#1344) only under `panic = "unwind"`; no fixed pattern has
reached it in the differential's matches.

Three regressions guard these facts:

- `fixed_patterns_match_as_ecmascript_does_through_every_keyword` runs each
  pinned pattern through `pattern`, `propertyNames`, `patternProperties` and the
  fused `patternProperties`/`additionalProperties` path, under the fixed-string
  presentation of unpaired surrogates, over a 60,000-string generated corpus,
  and compares every result with V8's `new RegExp(pattern, "u").test(string)`
  recorded in `crates/openbindings-internal-schema/tests/fixtures/fixed-pattern-ecma.json`.
  Its generator, `fixed-pattern-ecma.mjs`, regenerates the corpus from a seed;
  the TypeScript suite derives the recorded results from V8 again on every run.
  The test also requires the fixture's patterns to be exactly the pinned schemas'.
- `fixed_checks_run_their_patterns_on_the_default_engines_finite_automaton`
  pins the engine choice: lookaround compiles, so the default is still
  fancy-regex, and a valid version that would exceed fancy-regex's default limit
  of a million backtracking steps on its backtracking path still matches.
- The memory probe's near-miss phase (`tools/verify-diagnostics.py`) requires
  fixed-check peak bytes and allocation counts not to grow from 1 MiB to 8 MiB
  near misses of the SemVer, name, anchor and identifier patterns.

The source digests of jsonschema, jsonschema-regex and fancy-regex are recorded,
so an upgrade of any of them fails the isolation check until it is reviewed: run
the three regressions above, the 440-case specification corpus, and the document
validation differential against the previous release, then refresh the records.

## The default evaluator on the unmodified library

The evaluator compiles each prepared contract with
`jsonschema::options_for::<FlatJson>()`, format assertion off and retrieval
denied. What it changes, it changes in the schemas before the library sees
them or through the library's documented `with_keyword` and `PatternOptions`
APIs:

- Core's schema space projects the contract after the meta-schema check of
  every schema it reaches (a conformant document's own schemas passed that
  check when the document was validated): each reached schema becomes a
  definition of its own, `n<number>`, in one projected resource per original
  resource, and every subschema position a `$ref` to its child's definition.
  Core hands the evaluator these definitions directly; it writes them as JSON
  text only to apply the text's fixed bounds when the projection could reach
  them. A reference that identifies no schema refuses preparation and names
  the schema holding it.
- `missing.rs` stands in for resources the request does not supply. When the
  projection fails for want of one, preparation adds an empty resource at each
  missing URI and projects again, and every definition projected from a
  stand-in compiles to the SDK's `$obMissing` keyword, which stops the pass
  with no verdict naming the missing resource. A value whose evaluation never
  reaches a stand-in keeps the library's verdict, which cannot depend on the
  missing content where keywords decide by evaluating their subschemas. The
  library's `unevaluatedProperties` and `unevaluatedItems` validators do not:
  they read the structure of the schemas applying to the same instance to
  collect annotations, and would read a stand-in as annotating nothing. So a
  contract in which such a keyword's walk (through `$ref`, `$dynamicRef`,
  `allOf`, `anyOf`, `oneOf`, `if`, `then`, `else` or `dependentSchemas`)
  reaches a stand-in refuses preparation, whatever order the library
  evaluates keywords in. A reference to a fragment of a missing resource has
  no stand-in, and preparation refuses as before.
- `bundle.rs` bundles the projected definitions into the `$defs` of one
  resource (JSON Schema Core §9.3), so the library compiles each definition once,
  whatever path reaches it. It resolves `$dynamicRef` itself: a definition is
  specialized once for each dynamic context it is reached in, the context
  recording, for each dynamically looked-up anchor name, the outermost resource
  entered so far that declares it (§8.2.3.2). The bundle holds no
  `$dynamicAnchor`, so the library never resolves through the dynamic scope.
  Contexts hold only the names some `$dynamicRef` looks up, and only those
  they assign. A contract whose dynamic references would specialize more than
  100,000 definitions, add more than 16,777,216 units of JSON in specialized
  copies (one per value and per byte of its strings and member names), or hold
  more than 4,000,000 dynamic-scope entries across distinct contexts refuses
  with `program-specialization-limit`. The SDK bounds its own output and does
  not estimate the library's compiled memory.
- `operands.rs` registers the SDK's keywords: exact numeric bounds and
  `multipleOf`, `const` and `enum` as handles into the exact literal arena,
  `pattern`, the `$obMissing` stand-in, and, only when the application sets an
  evaluation work cap, the `$obWork` marker the bundle places before each
  reference.
- `pattern.rs` reads every pattern as an ECMA-262 11th edition `u`-mode pattern
  and writes a form in the regex crate's syntax meant to have the same matches,
  spelling each character set out as the positive class of the scalar values
  it matches: the regex crate's negation of a class whose ranges meet across
  the surrogate gap adds a range across it, so no form holds `[^`. The empty
  set is two contradicting ASCII word-boundary assertions, since
  jsonschema-regex escapes class set operations. A form longer than
  `pattern_bytes` is not written out, and each compiles within what the
  contract's `total_pattern_memory` gives it, set through the regex crate's
  documented `size_limit` (48 KiB first, at most 32 MiB) and `dfa_size_limit`
  (at most 2 MiB), for the library's `patternProperties` names through
  `PatternOptions::regex()`, which takes one setting for every name: when a
  name needs more than 48 KiB, the SDK compiles each name's form itself to find
  what it is given, and the library compiles them within the largest, relying
  on a form compiling to the same program within any size limit it fits. A
  seeded differential against V8, with fixed rows across the
  surrogate gap, checks both keywords. The `pattern` keyword
  matches through that form, compiled on first use, or through a run matcher
  for one anchored sequence of single-character runs. `patternProperties`
  names are written in their forms, which the library compiles with
  `PatternOptions::regex()` and jsonschema-regex's translation leaves unchanged.
  The word boundary `\b` becomes the regex crate's ASCII word boundary, which
  reads word characters as ECMA-262 does and never falls inside a multi-byte
  character. Its complement does, and the regex crate's `is_match` then
  answers no for a pattern another branch matches (`\x{E9}|(?-u:\B)` on
  "aéa", where `find` matches), so `\B` is not matched. A pattern holding
  `\B`, one that only a backtracking engine can match (lookaround or a
  backreference) or one with a Unicode property escape has no form: a string
  that reaches it in `pattern` gives no verdict, and as a `patternProperties`
  name it refuses preparation. Each keeps its own reason.
- `depth.rs` bounds the schema objects any evaluation path can hold at once,
  from the bundle's reference graph, as the lesser of two linear bounds in
  the value's depth. Before evaluating a value, the evaluator compares that
  bound with the nesting `stack_bytes` holds at 256 bytes each (`stack.rs`) in
  constant time (from the value's node count when that suffices, otherwise
  from its depth). The library's unevaluated validators take frames the bound
  does not model, so a contract holding one is also guarded as it evaluates:
  `openbindings-internal-json/src/work.rs` checks, at every read of a child
  instance, the stack the pass has used since it started, and fails every read
  once it is more than half of `stack_bytes`, so the library stops descending;
  `tests/evaluation_stack.rs` checks 39 shapes on threads of 192 KiB, 256 KiB
  and 1 MiB, and a library upgrade reruns it. A contract that could recurse
  without consuming the instance refuses (`evaluation-cycle`), and one whose
  unevaluated keyword's walk weighs more than `stack_bytes` admits refuses
  (`unevaluated-walk-limit`), since the library compiles the walk by
  recursion: `bundle.rs` weighs each reference by the keywords on its path,
  `stack.rs` sizes the limit (a fifth of the stack left to the caller, 99 KiB
  to the rest of preparation, 1 KiB per unit), and `tests/walk_stack.rs`
  prepares the longest admitted walk of each kind below enclosing schemas on
  threads of 192 KiB, 256 KiB and 512 KiB with a fifth of each already in use,
  so a library whose compiled walk frames grow fails it. A library upgrade
  that changes those frames remeasures the reserve and the unit on every host
  (the method and per-host tables are in `docs/performance-boundaries.md`).
- `explain.rs` maps the library's first error back to its keyword's original
  schema location.
- `openbindings-internal-json/src/work.rs` holds the optional evaluation
  allowance and the stack guard. `FlatJson` charges its reads only under that
  crate's `evaluator-work` feature, which only the evaluator enables, and only
  inside a pass that has an allowance or a guard.

Core and the evaluator share one copy of the library and never exchange a value:

- `openbindings-internal-schema` and `openbindings-internal-json` expose no
  library type. The evaluator reaches core's schema space, with its projections,
  reference resolution and meta-schema checks, through SDK types only, and
  builds its own validator from the bundle. `FlatJson` implements the shared
  jsonschema-value traits once for both.
- `openbindings-internal-json/src/numeric.rs` holds only exact arithmetic; each
  side registers it: core's fixed checks the four comparisons, the evaluator
  all five keywords.
- Core opens no work scope, so within a workspace build, where the feature is
  on, its reads still count nothing; core's own graph, built alone or by a
  consumer of core alone, holds no evaluator.

Refresh both attribution inventories and notices offline with
`python3 tools/attribution.py`; verify with `--check`. The native inventory is
the union of the closures for Linux (GNU), macOS and Windows (MSVC) on x86_64
and aarch64, so a dependency one of them alone pulls in is attributed; fetch
every platform's sources first with `cargo fetch --locked`.

## The unevaluated-walk transform

`src/unevaluated_walk.rs` is a stopgap for an upstream bug in jsonschema
0.58.6. To build an `unevaluatedProperties` keyword, the library walks the
schemas that can evaluate the instance's members, following `allOf`, `anyOf`,
`oneOf`, `if`, `then`, `else`, `dependentSchemas` and references. For each
`patternProperties` it reaches, `compile_pattern_properties` in
`keywords/unevaluated_properties.rs` compiles every subschema and then drops
the compiled node, keeping only the pattern. A `$ref` target first compiled
there can be one that a later compilation round still points at through a
non-owning edge; validation then reaches the dropped target and panics with
"pending schema target dropped", which aborts a WebAssembly host. As of release
0.59.0 the walk still compiles and drops the subschemas.

The transform keeps every subschema out of that walk. In each bundled
definition the walk can reach, `patternProperties` keeps its patterns, each
with the schema `true`, and the original subschemas apply from a definition of
their own, `{"not": {"not": {"patternProperties": ...}}}`, which the
definition's `allOf` references. The walk never enters `not`, so it compiles
only `true`, and the subschemas are compiled once, owned by their `not`.
Validity is unchanged (`not` twice is the schema itself), and so are the
evaluated member names, which the walk still reads from the patterns left in
place. An explanation whose first error lies in a moved subschema names the
original `patternProperties` keyword, at the object holding the member.

The transform depends on which keywords the library's walk follows. The
evaluator's `unevaluated_walk` tests compile a bundled program without the
transform and require the library to panic on it, and require the walk still to
skip `not`. The reproducer's four placements run with the transform, natively
and in the Wasm suite, and `reference_placement` compares 500 seeded random
contracts, split across document, contained and supplied resources, with the
same schemas in one resource. A library upgrade that fixes the bug fails the
first test: remove the transform and its tests in that upgrade. One that makes
the walk enter `not` fails the second: the transform must change before the
upgrade lands. The benchmark's `unevaluated-patterns` workload guards the
transform's retained heap.

## Advisory policy

The required `advisories` job in [CI](.github/workflows/ci.yml) runs
`cargo deny --workspace --all-features --locked check advisories` on pull requests
and pushes to `main`. It checks the workspace lockfile, including optional native
TLS dependencies. Vulnerabilities fail cargo-deny's advisory check; [deny.toml](deny.toml)
also denies yanked versions. The aggregate `ci` check requires this job to pass.

There are no ignored advisories. Any exception must identify a RustSec advisory
and include a written justification in an `{ id = "...", reason = "..." } entry.
Bare IDs and crate/version ignores are forbidden. CI and the local source replay
enforce that format with `python3 tools/verify-advisory-policy.py --self-test`.
The advisory check needs network access to current RustSec and registry data;
offline policy validation does not establish that dependencies are advisory-free.

## Numeric ownership and consumer features

Exact Decimal arithmetic belongs to `openbindings-internal-json/src/decimal.rs`.
It retains the normalized coefficient and signed decimal exponent implementation
and its tests; no power expansion occurs. Divisibility has the same finite
coefficient/exponent admission and explicit arithmetic-limit refusal. Public
semantic equality, preference validation, adapter integer classification and
uniqueness share this implementation.

`numeric.rs` holds exact comparisons and division; each library's adapter
registers them with its documented `with_keyword` / `Keyword` APIs. The
evaluator's `operands.rs` replaces numeric bounds with positive handles and
const/enum with handles into exact flat storage.
Every size operand, including minContains/maxContains, is normalized to a
nonnegative u64, saturating only above the physically realizable collection
size. Native contains and its annotations remain the library's responsibility.
Type and uniqueItems use documented JsonNumber::is_integer and Array::is_unique
overrides; fixed schemas use exact comparisons with their trusted small integer
operands, and assert no `multipleOf` (a test checks every position).
Original schemas are validated before substitution.

Inspection resources substitute numeric leaves without changing their shape.
Their pointer-identity table is built alongside each stable boxed copy, paired
structurally with original occurrences, never inferred from JSON equality.
Runtime diagnostic schema facts use the projection's original-source provenance.

The private FlatJson adapter redacts cold Node::to_value/JsonNumber::to_number
payloads. These documented cold APIs cannot represent every exact JSON number.
They are never the source of a verdict or a reported instance/schema fact:
equality, uniqueness, integer classification and numeric keywords have exact
implementations. The SDK does not expose library error Display/Debug or payloads. An
explanation reads only the library error's kind, instance path and keyword
location, and words its message as fixed text for that kind. The cheap check
never requests library errors. An explanation retains the library's documented
allocation exposure; building the error takes about twice a check's stack per
level, so the evaluator explains a value only within half its evaluation depth
limit.

Shared serde_json enables raw_value and unbounded_depth for exact RawValue
interoperation and admitted schema decoding. Upstream jsonschema/value additionally
enable float_roundtrip. The SDK never enables arbitrary_precision. Consumers may
explicitly enable it on the same identity; Number protocol admission remains
checked and preserves only the spelling/value the consumer actually emits.
Ordinary Serde flatten/untagged behavior is covered by the external consumer tests.

Shared runtime version floors remain Serde 1.0.229, serde_json 1.0.151,
itoa 1.0.18, fluent-uri 0.4.1, url 2.5.8, reqwest 0.13.5, Tokio 1.54.1 and
bytes 1.12.1. The relocated arithmetic uses num-bigint 0.4.8 and num-traits
0.2.19; both floors are included in consumer resolution checks. SDK packages, jsonschema, jsonschema-value and wasm-bindgen
remain exact at their declared versions. The workspace lock fixes qualification.
`tools/verify-consumer-compat.py` checks twelve configurations: baseline/AP,
core/AP, evaluator/AP, portable discovery, all three interfaces, and native discovery/AP.
It asserts one serde identity using actual normal/build feature trees and records
package counts. Run its `--resolution minimum` and `--resolution latest` modes for
dependency updates, retaining `--output` receipts. Offline latest means newest
cached resolvable graph, not a crates.io-head claim.

## Invariants and regression gates

Keep exact input in the SDK's flat immutable arena. Do not convert values through
f64 or serde_json::Value as an authoritative convenience path. The private
compiler projection can strip annotations only because the declared evaluator is
a validity evaluator with format assertion disabled; original SDK documents and
diagnostics preserve original content. The evaluator's const/enum and numeric
keywords preserve exact values.

Core resources are explicit. The evaluator denies retrieval; static inspection
uses a recording retriever with inert resources that never supply a verdict, and
the fixed checks retrieve only their own local resources. A host enabling
dependency HTTP/file features on the schema library must still fail to produce
any HTTP/FIFO trap hit through the SDK; direct positive controls on the library
prove the traps work.

The recorded source digests make the isolation check fail on any dependency
upgrade until it is reviewed. Before adopting one:

1. Read the upstream changes against what the SDK relies on: the documented
   `with_keyword`, `PatternOptions` and build APIs, the walk the
   unevaluated-walk transform depends on, jsonschema-regex's translation of the
   evaluator's pattern forms, and the regex crate's syntax and Unicode
   semantics.
2. Run the unevaluated-walk tests and act on either failure as above.
3. Re-run the full pinned 440-case OpenBindings corpus and the evaluator kit's
   1,564 observations, with its exact-case refusal and capability declarations,
   natively and in real Wasm (`python3 tools/verify.py --browser`), and the
   pattern differentials: `pattern_correctness` checks seeded generated
   patterns and lone-surrogate escapes against results recorded from V8 through
   `pattern` and `patternProperties`, and the TypeScript suite derives the
   generated rows' results from V8 again; core's fixed-pattern regressions are
   above.
   Run the no-I/O feature-unification traps, dynamic-scope, cancellation, deep
   value, exact arithmetic and stack tests. The release stack test re-checks
   the depth bound's frame model, the stack guard's headroom and the walk
   limit that keeps preparation on a 256 KiB thread: a library whose frames
   grow fails it, and the guard's budget or the walk limit changes with the
   library. The missing-resource tests check stand-in verdicts against every
   completion.
4. Repeat the frozen representative performance and memory series
   (`tools/evaluator-bench`, `tools/verify-diagnostics.py`) and the packed
   consumer tests. A verdict becoming a refusal is a regression unless the
   governing authority intentionally changes; update scope only through an
   explicit separate decision.
5. Refresh RustSec and license inventories, then the source digests in
   `docs/dependency-isolation.json`, only after review.

The maintained SDK exact-operand, ECMA, resolver, diagnostic, corpus and kit
tests govern this supported graph. Re-run upstream source suites when updating
the corresponding library and investigate each changed result explicitly.
