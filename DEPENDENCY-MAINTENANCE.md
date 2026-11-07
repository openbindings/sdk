# Internal dependency maintenance contract

Core and the default evaluator both use the unmodified jsonschema 0.58.6
library from the registry, under its own name. Core keeps the library's default
pattern engine (jsonschema-regex 0.58.6 and fancy-regex 0.19.2) for its fixed
checks (OBI-02 and OBI-10) and uses the library for the resource identity and
reference resolution of its reference inspection. The evaluator matches schema
patterns with immutable regex-automata 0.4.18 NFAs or its own simple matcher,
using regex-syntax 0.8.11 after strict ECMA interpretation. jsonschema-value 0.58.6, referencing 0.58.6 and
serde_json 1.0.151 are unmodified too. The SDK carries no fork, vendored crate
or patch of any dependency: every adaptation is SDK code, a fixed-schema transformation, or a documented
extension point. User schemas are evaluated at their original locations.
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
256 KiB thread). The library catches regex's known search panic
([rust-lang/regex#1344](https://github.com/rust-lang/regex/issues/1344)) only under
`panic = "unwind"`. The pinned SDK settings make that panic unreachable for
customer patterns as well as the small fixed patterns; this does not depend on
unwinding on wasm32.

The meta engine exposes the bounded backtracker only for a span no longer than
`max_haystack_len()`. With its default 256 KiB visited bitmap, the wrapper's
unwrap can fail only for an empty span and more than 2,097,152 final NFA states.
The SDK's `COMPILED_SIZE` is 32 MiB. The pinned Thompson builder charges every
builder state plus its heap payload and rejects excess before returning an NFA;
conversion never increases state count. Builder states are 32 bytes on native
64-bit and 16 on wasm32, so even ignoring payload the respective upper bounds
are 1,048,576 and 2,097,152 states. The latter is inclusive and the panic needs
strictly more. Final states are 24 and 20 bytes respectively; those sizes are
not the sizes charged by builder admission.

Actual customer admission is much tighter. Original preparation first requires
at most 65,536 dispatch pressure and 1 MiB helper payload per pattern, together
with its AST and translation profile. Limited programs cannot select compiled
evaluation. Compiled admission also excludes names containing `{` and excludes
such value patterns without a Simple matcher. Searching concatenations and
alternations of Unicode classes, astral ranges, dot and negated classes found
180,332 final states for 16,382 literal `[U+0081-U+80001]` classes concatenated
without quantifiers. That is a measured maximum among the searched shapes,
not an exhaustive maximum over patterns. Its margin is 1,916,820 states on both
native and wasm32. A nullable variant of 16,379 classes followed by `|` has
180,300 states, a margin of 1,916,852, and exercises an empty match without a
minimum-length rejection.

`compiled_qualification::regex_search::dense_customer_patterns_keep_empty_search_below_backtracker_capacity`
counts the normalized forms with the Thompson configuration used by
`regex::RegexBuilder`: UTF-8 syntax and empty matches, forward construction,
all captures, no shrinking, and the same 32 MiB size limit. It checks the
bounded backtracker directly and, through public preparation, checks an empty
string and an empty member name for both patterns. Route counters require
compiled construction and evaluation. One more class must select Original
before backend entry. `packages/typescript/test/regex-search-boundary.test.mjs`
repeats the public searches in Wasm. State counts are target-independent for
these forms and pinned compiler settings; builder layout, memory usage, stack
headroom and panic behavior are target-dependent. Recheck the argument, sizes,
counts, admission boundaries and both tests whenever any regex-family dependency
or the Original pattern profile changes. The evaluator pins regex exactly to
1.13.1 alongside regex-automata 0.4.18 and regex-syntax 0.8.11.

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

## The default evaluator and dependency boundaries

The default evaluator prepares one immutable plan over original schema locations.
It does not compile a projected user schema through jsonschema. Original resource
identity, reference queries, dialects and dynamic scope remain SDK-owned. Explicit
heap frames apply concrete keywords; bounded symbolic reasoning handles undefined
predicates, missing content and incomplete capabilities without conflating them.

- `semantic/construction.rs` shares preparation ownership across the root plan and
  selected meta-schema plans. Completed syntax and reference facts are reused.
- `pattern.rs` reads ECMA-262 Unicode-mode syntax, distinguishing invalid syntax
  from valid unsupported features. A narrow anchored ASCII literal path produces
  the same complete reading as the general Reader.
- `semantic/patterns/preparation.rs` indexes normalized forms by length and hash,
  then checks exact text within collisions. Equal programs share immutable owners;
  their semantic effects and original locations remain separate.
- `semantic/patterns/compiler` admits finite structural AST/HIR/NFA bounds before
  one forward capture-free NFA build. There is no geometric compilation retry.
  Local supported-profile failures become retained limitations; aggregate quota
  exhaustion aborts preparation. The current resource contract is recorded in
  `docs/alignment-checkpoint/compiler-resource-policy.md`.
- `semantic/patterns/cache.rs` owns per-call search caches. Work reservations include
  outgoing transitions and epsilon breadth. Immutable programs contain no mutable
  search state. Reader, structural, NFA, cache and original-effect controls must be
  rerun when upgrading regex-syntax or regex-automata.
- Exact numbers use `openbindings-internal-json/src/numeric.rs`. Inline signed
  integers avoid decimal scratch allocation; wider operands keep exact decimal
  arithmetic and its explicit work/storage limits.
- `semantic/execution` evaluates applicators with explicit frames. Reference-free
  plans need no completed-application memo; reference plans retain it for coherent
  reuse and recursive refinement. Annotation identity and value identity remain
  separate.
- `semantic/explain.rs` materializes bounded original-location witnesses after a
  verdict. Fixed SDK messages never include authored values. Diagnostic exhaustion
  preserves an established failure with incomplete problems.
- Symbolic proofs have a private maximum of 64 distinct effects. A separate sampled
  stack guard is a refusal check, not a portable physical-stack guarantee. Actual
  native and Wasm stacks require qualification for each build.

The evaluator still uses jsonschema for fixed SDK-owned format validators, with
retrieval denied. Core uses it for the fixed document/local meta-schema checks
above. Neither path compiles arbitrary user schemas as an evaluator substitute.
`FlatJson` implements the shared jsonschema-value traits without exposing upstream
library types through public SDK APIs. Internal JSON reads observe work only in
an evaluator scope; core-only consumers do not acquire the evaluator dependency.

The outgoing bounds, bundle, cycle-unfolding, stand-in, depth-estimation and
unevaluated-walk transforms are deleted. Their historical measurements describe
older revisions, not requirements to preserve those implementations or quotas.
The original unevaluated-placement workload remains a semantic regression test.

Refresh both attribution inventories and notices offline with
`python3 tools/attribution.py`; verify with `--check`. The native inventory is the
union of closures for Linux (GNU), macOS and Windows (MSVC) on x86_64 and aarch64,
so a dependency pulled in by only one platform is still attributed. Fetch every
platform's sources first with `cargo fetch --locked`.

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
   library. The missing-resource and stand-in tests check stand-in verdicts
   against every completion.
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
