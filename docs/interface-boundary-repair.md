# Interface boundary repair — 2026-10-12

This record supersedes the interface-authority claims in the inspection, synthesis
and invocation checkpoints. Their execution evidence remains historical evidence
for the identified SDK revisions. Passing a checker against a draft amended to
match that checker did not establish that the amendment belonged in the interface.

## Scope and baseline

Reviewed interfaces commits `8ff5d9d`, `9e8fcff`, `56aadc0`, `3972eb9` and `8b25ddc`
against `3518167ef0f9f78e13688c32325cab602b80f965`. The repair also removes older
implementation instructions in the same four role publications where they would
contradict the repaired boundary. Other roles, binding kinds and the core model
are not redesigned. SDK baseline is `0a3bc18`.

Work is on `codex/interface-boundary-repair` in interfaces and
`codex/sdk-interface-boundary-repair` in the SDK. Interfaces repair revision: `468bb83e5986de3ee490767bcb4b6630071bdec5`. No publication or merge is implied.
The core document model, SDK runtime algorithms and public Rust signatures are
unchanged by this repair.

## Decisions

| Concern | Publication | SDK |
| --- | --- | --- |
| Adoption and authority | Operation names, value shapes and capability meanings; voluntary per-operation correspondence remains an author claim. | Its native API has its own documented provider and consumer requirements. |
| Inspection identity | Source-local target meaning and cross-role reference meaning remain. No unique-row or rejection requirement. | Exact duplicate rejection and checked serialization remain SDK policy. |
| Malformed carriers | Core field meaning and schema acceptance are distinguished; no prescribed recovery. | Strict admission before provider work remains. |
| Inspection ordering | No byte/order determinism is imposed by idempotency. | Stable provider ordering remains a native expectation. |
| Synthesis dependency attribution | `dependencyKey` is optional information, not a required bijection. | Represented/lossy dependency entries require it; exact accounting remains checked. |
| Durable source attribution | `sourceKeys` is optional. When present its input-order mapping has defined meaning. | Always derived and emitted; each input is preserved and mapped. |
| Unbound generated operations | No added ban; the core permits abstract and dependency-only operation declarations. | Checked synthesis retains the binding-or-dependency policy. |
| Completeness flags | Describe the result's claim. Strict success describes complete synthesis of admitted callable targets. | The SDK computes flags and strict eligibility from one checked coverage result. |
| Coverage duplicates | No mandated identity tuple or reject/merge policy. | Existing exact identity and duplicate checks remain. |
| Source handling and composition | Emitted sources preserve supplied values and supplied names. Unused inputs need not be emitted. | Every input is preserved; conflicts and unsupported composition are rejected according to native policy. |
| Source-free synthesis | Describes a source-free skeleton with caller metadata. | Existing checked empty skeleton path remains. |
| Naming, grouping and schemas | No one-target/one-operation algorithm, compulsory reference inlining or byte-stable generation. | Providers choose their strategy within SDK result checks. |
| Frame exchange | Input/output/closure and terminal message meanings remain. A value alone is not successful completion. | Native queues, polling, admission, first-settled terminal and draining rules remain. |
| Invalid frames and late inputs | No mandatory parser order, error precedence or recovery algorithm. | Strict test-bridge recovery remains explicitly SDK policy. |
| Cancellation | Cancellation reports no rollback or external-stop guarantee. | Existing cancellation propagation, outcome and owner-release guarantees remain. |
| Context | Challenge, requirement-family, confidentiality and reuse meanings remain; no store, trust algorithm or retry loop is imposed. | Existing host-owned context model remains; no broker added. |
| Boolean configuration schemas | Object and Boolean schema values remain expressible. | Existing handling remains. |
| Operation selection | Explicit and ordered choice meanings, core name resolution and error meanings remain. | Candidate construction and malformed-document error precedence remain SDK policy. |
| Operation admission and validation | Input is an OBI carrier; no claim of a conformance certificate or internal preparation strategy. | Readable partial document admission and lazy evaluation remain. |
| Tests | Interface examples test shapes and illustrate meanings, including permitted variation. | Twenty-six existing synthesis cases are preserved as SDK policy regressions. |

Optional attribution preserves a useful output vocabulary without making it the
price of implementing the interface. Absence means the consumer has less evidence;
it does not mean the producer violated the interface. This SDK's checked native
result is a stronger product than arbitrary remote JSON matching that interface.

The repair is substantive: it changes draft descriptions, makes two attribution
fields optional and relaxes added restrictions. It is not just replacing MUST
with SHOULD or adding a disclaimer around the same algorithms.

## Evidence ownership

`tests/fixtures/interface-synthesizer-0.2.json` and the other role JSON files are
pinned interface snapshots. The new `interface-output-cases.json` mirrors shared
shape examples. They include duplicate evidence, absent attribution, additional
report fields and unbound operations; malformed field types still fail schema
checks. A Rust test evaluates the complete returned envelope using the actual
published output schema and the SDK's evaluator.

`coverage-cases.json` and `coverage-fixture.schema.json` now identify
`openbindings.sdk-synthesis-policy-cases@1`. All 26 earlier cases are retained;
they are neither re-exported as universal obligations nor weakened to hide a
regression. An inspection test contrasts permitted result shapes with this SDK's
stricter duplicate rejection. Invocation frame-shape and native lifecycle tests
retain their separate scopes.

Historical qualification receipts are not rewritten to claim a new run. The
repair receipt records the checks actually rerun for this change. No new browser,
network-adapter, performance or cross-implementation qualification is claimed.

## Verification result

The [repair receipt](qualification/interface-boundary-repair.json) records the
qualified SDK revision and exact interface hashes. All 93 affected native tests
and four associated doctests passed. Interface checks cover all eight documents,
shared schemas, comparison-profile membership, canonical order, fixture schemas,
embedded documents and the pinned core schema. Strict Clippy and the public Rust
reference/doctest checks passed. All three external-consumer qualification tools
passed against extracted Cargo archives from the clean SDK repair commit.

No runtime algorithm changed, and no new performance claim is made. The next
adapter slice can build on these provisional native APIs without promoting their
implementation policies back into the published interfaces.
