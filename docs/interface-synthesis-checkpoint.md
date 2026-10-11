# Interface synthesis checkpoint — 2026-10-11

Implementation is on `codex/sdk-interface-synthesis`; the coordinated draft role
is on `codex/interface-synthesis-contract` in `openbindings/interfaces`, revision
`9e8fcff80643ff4ef30415dc782489db65789ef9`. This is a local, provisional Rust foundation
slice. These branches have not been merged, published or deployed to the website.
The TypeScript/Wasm consumer is a qualification bridge, not a released role API.

## Capability and authority

`openbindings-interface-synthesis` adds one optional provider trait and one checked
consumer facade. Inspection is not a dependency. A provider may implement both
roles over shared private analysis, or either independently. Core document semantics
remain responsible for conformance; kind interpretation, acquisition and truthful
inventory/soundness remain provider responsibilities.

The role was clarified before implementing its checks:

- **C1:** represented/lossy dependencies explicitly name `dependencyKey`. Emitted
  dependencies and their coverage rows correspond one-to-one; equal counts alone
  are insufficient. Dependency rows cannot claim source/operation/binding paths.
- **C2:** strict eligibility requires exhaustive coverage and no unsupported unit
  in any scope. Full representation instead requires every disposition to be
  represented or invalid. Excluded and sound lossy units can pass strict synthesis.
- **C3:** coverage identity is source index + scope + source reference; projections
  additionally include emitted path keys. Duplicate/conflicting identities refuse,
  including in partial reports. Equal references across inputs remain legitimate.
- **C4:** absent/empty sources request an exhaustive skeleton. Every supplied source
  is preserved, every output source corresponds to an input, explicit names are
  honored, and conflicting names refuse before work. Equal values may share a key
  where names permit, while retaining each input's coverage index.

The [crate documentation](../crates/openbindings-interface-synthesis/README.md)
maps all four role operations and fields to Rust. Role JSON, 17 portable coverage
cases, fixture schema, authority revision and SHA-256 hashes are retained under
[tests/fixtures](../crates/openbindings-interface-synthesis/tests/fixtures/README.md).

## Consumer behavior

Providers return editable `SynthesisCandidate` values. The facade validates core
conformance, request overrides, exact source preservation, coverage obligations,
unit identities and output links before constructing immutable `Synthesis`.
Strict calls yield `ValidatedDocument`; coverage calls retain document and report.
An `Incomplete` error retains that same checked result for one-call recovery.
Invalid output instead retains its candidate, assessment and located finding.
No unsound/unknown core document is promoted to a partial success.

Sources and requests are borrowed. Result owners survive their source/provider
handles. Borrowed, Arc and Rc providers forward their associated futures without
requiring cloning or boxing. Native Send and stack-borrowed unboxed futures are
qualified, as are local Rc futures across a real JavaScript Promise. Cancellation
wakes waiting calls and drops their work; it cannot preempt a blocking provider poll.

Exact envelope persistence retains opaque values, content absence/null, Unicode
and large integer tokens. Bounds apply only to the selected encoding operation.
Ordinary success Serde and the exact encoder share the checked wire shape. Reloading
or editing JSON does not fabricate a new checked coverage claim. Core still refuses
to prove conformance for unpaired UTF-16 documents; that Undetermined assessment is
retained. Opaque report evidence can preserve those units without rewriting them.

## Acceptance coverage

| Case | Maintained evidence |
| --- | --- |
| S01 | `synthesis-only` external test: source-free metadata/version overrides, strict/coverage agreement |
| S02 | External shared-provider test: exact source name/content, metadata and one observed state |
| S03 | `admission_is_lazy_and_precedes_provider_work`: kinds, carrier, version, names, conflicts, zero work |
| S04 | External composition test: both sources retained; arity refuses before analysis |
| S05 | Source-index test: repeated equal named/unnamed inputs share a source; external distinct names stay distinct |
| S06 | External disposition test: all 20 scope/status combinations; independent portable truth table |
| S07 | Boundary failures and strict recovery: missing/empty limitation refuses; unknown remainder stays explicit |
| S08 | External multi-source unreadability and malformed-catalog tests: no empty/partial fabrication |
| S09 | External recovery and forged-error tests: checked partial from one call, provider cannot forge recovery |
| S10 | Core-invalid/version/Undetermined candidates retain separate evidence and never become partial success |
| S11 | Located boundary cases check source index, binding, operation, source links and actual input correspondence |
| S12 | Portable dependency cases: valid/lossy, duplicated/missing/nonexistent keys, wrong scope/path |
| S13 | Duplicate-unit negatives; repeated source references across inputs and projections across paths |
| S14 | Independent catalog semantics test: target identities agree with inspection; neither role calls the other |
| S15 | External non-Clone borrowing/Arc, conditional Send and stack-borrowed unboxed future tests |
| S16 | Extracted-package Wasm/strict TypeScript probe: Rc across Promise; handles dropped before encoding |
| S17 | Lazy, pre-cancelled, two-waiter wake/drop and abandoned-future tests |
| S18 | Exact boundary and host probes: absence/null, Unicode, integer tokens, extensions and opaque report evidence |
| S19 | Exact persistence test: Serde agrees; exact byte limit and insufficient byte/node/depth limits |
| S20 | Envelope reparse and authoring edit test; original evidence remains attached to its immutable snapshot |
| S21 | Core alone; synthesis-only resolved graph excludes inspection; external archive consumption |
| S22 | Fixed 0/100/1,000/10,000-unit release measurements, one-call counters and indexed correspondence |

The catalog's independently stated soundness/admission rules and deterministic
naming policy are in its [README](../examples/interface-synthesis-consumer/README.md).
The fixture is deliberately small and claims neither general projection nor execution.

## Review and qualification

One GPT-6 Astra xhigh reviewer examined the contract, native seam, ownership,
validation, consumer and host path. Three initial blockers were repaired with
regressions: forged strict recovery, unpaired-UTF-16 comparison aliasing, and an
empty optional `sourceKey`. Typed request version evidence was also retained.
The reviewer independently reran seven boundary tests and reported no remaining
material blocker. A subsequent narrow review of the measured core optimization
and source-name admission also found no semantic regression or blocker. It did
not independently rerun the archive, host or performance measurements.

Local checks cover crate tests/doctest, the pinned output schema and portable fixture
schema, external consumers/recovery, workspace Clippy/tests, rustdoc missing-doc and
broken-link gates, independent core build, prior inspection consumer, verifier
receipt regressions and dependency isolation. The workspace run passed 271 tests with two existing ignored tests. The existing
HTTP discovery tests passed after rerunning with permission for local listening
sockets. The final synthesis suite passed 12 checks including its doctest; the
external dual-role consumer passed six tests, and the isolated synthesis-only
consumer passed its metadata/ownership test.
CI wiring was updated, but local execution is not a claim that remote CI has run.

The native production dependencies reuse existing core, Serde and exact JSON
support. There is no new runtime/HTTP dependency. Tokio/evaluator are dev-only.
Final archive receipts include SDK HEAD, cleanliness, applied interface revision,
archive hashes, toolchain and successful commands in
`target/interface-synthesis-qualification/archives/receipt.json`.

## Measured core repair

The fixed workload revealed repeated linear primary-key scans in core assessment,
causing quadratic binding-reference checking. Borrowed key sets now check primary
operation/source membership once per reference. Existing malformed values still
count as present keys; aliases remain excluded; finding order and locations are
unchanged. A dedicated core regression and broader core/workspace tests cover this.

At 10,000 units the local median complete call dropped from **300.6 ms to 39.5 ms**;
cold assessment dropped from **269.9 ms to 10.7 ms**. New cached coverage checks took
**9.2 ms** and optional exact encoding **21.2 ms**. See
[measurements, spreads and guardrails](qualification/interface-synthesis/README.md).
These measurements establish this fixture's baseline, not general performance grades.

## Remaining adapter questions

Binding invocation is the next role slice. The future OpenAPI adapter still needs
to establish version-specific target/alternative/dependency inventory, stable
source references, deterministic naming, sound schema projection, source resource
acquisition/refusal, and runtime prerequisites. Those semantics can challenge the
provisional interfaces with concrete cases. No OpenAPI client behavior, binding
specification or invocation engine was added in this slice.
