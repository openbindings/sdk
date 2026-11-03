# Saved SDK 0.2 alignment checkpoint

This is a feature-branch recovery checkpoint requested by Matthew. It deliberately
includes unfinished source-pull work. It is not a merge, release, whole-workspace
qualification or completed-alignment claim. Continue on `codex/sdk-02-foundation`;
existing unpublished APIs have no compatibility protection.

## Authority and reviewed checkpoints

- Specification: `c1419c9fc3ae5a00c2b83da710c11bf67dab7934`.
- Interfaces: `5815fb106bddd2c85ff4e93e0077570b3f6b9e74`.
- `f1875f4`: bounded recursive evaluation, annotation refinement and corrected
  identifier-dependence adjudication. Original auxiliary expectations unchanged.
- `8ac089e`: current kind descriptions, source checking and shared cancellation.
- `42cd6f0`: bounded exact schema identity against the pinned comparison profile.
- `a7e4548`: Matthew's requested safety checkpoint, including the carrier foundation.
- `574dc05`: independently reviewed shared finite-provider cancellation lifecycle.

Canonical owned document entries, exact JSON, conformance, original schema context,
major evaluator semantics and current context/invocation APIs are implemented.
Comparison passes 14 ordinary and 10 exact identity groups, 64 independent controls,
204 independent cancellation checkpoints and 80 allocation lanes. Both supported
Rust toolchains and Wasm builds pass for that companion. The 88 directional groups
are outside its claim. These overlapping inventories are not additive coverage.

## Source-pull implementation saved here

`openbindings-source-pull` implements exact additions, reports, entries, proposals,
original-document requests, provider/client APIs and offline result validation.
The checks cover key/alias collisions, source attribution, exact binding/report
correspondence, existing-binding coverage, uses-label existence and known closure.
All additions are validated in a prospective snapshot; proposals remain unapplied.
The shared finite-call helper preserves lazy dispatch and cleanup cancellation.

Current tests cover all 21 unmodified pinned pull-report cases, five published
example pairs and 74 independent design controls. Duplicate uses are refused at
the earlier carrier gate rather than the later relationship gate. Two illustrative
kind implementations project from source alone, then use profile identity for
reuse/conflict and existing-entry drift. Consumer witnesses cover inspection,
explicit selective adoption, revalidation, repeated pull and explicit proposals.
Comparison failure and unrepresentable removal-only drift remain operational
inability. Core owns preference/idempotent on bindings, despite loose wording in
the source-puller prose; operation descriptions and binding author fields survive.

[Independent implementation review](alignment-checkpoint/source-pull-review.md)
passes at the recorded source hashes, with 500 additional closure cases, 48
allocation lanes and cancellation/ownership controls. Its annotation-drift finding
in the illustrative provider is resolved. Legacy inspection/synthesis
crates and external consumers have not yet been retired. Core prospective
edit/validation internals retain core limits and are explicitly outside the pull
relationship ledger; this is not a whole-call allocation or interruptibility claim.

The separate source-pull consumer now passes against checkout sources and extracted
Cargo archives, with real async Wasm/TypeScript/Node calls on both Rust toolchains.
Its borrowing test found an implied-static Send requirement in all four finite
clients. The [reviewed correction](alignment-checkpoint/borrowed-provider-review.md)
selects the provider future at the actual borrow lifetime before constructing
private async state. No blanket Send/Sync/static requirement was added. Current
source-pull tests total 26 groups/doctests; the related kind/pull/cancellation suites
pass 53 checks on both supported toolchains. Strict lint runs on the development
toolchain; the MSRV compatibility receipt explicitly records no matching Clippy run.

## Resume implementation

1. Source-pull implementation and external packaged/host consumers pass. Complete
   the current invocation consumer and remaining current-spec catalog/performance
   witnesses, then delete obsolete inspection/synthesis crates, support records,
   verification tools and old consumers. Historical APIs, test counts and incidental
   behavior have no compatibility protection.
2. Close ordinary compiler allocation accounting. The independent construction
   design calls for one attempt through SDK conversion/projection/diagnostics,
   early semantic routing for uncertified shapes, and a version-locked allocation
   schedule before enabling each backend lane. Do not substitute an unproved
   per-node multiplier or relax semantic/performance expectations.
3. Finish remaining invocation/discovery consumers and TypeScript organization,
   full workspace/host/CI/package checks, matched performance measurements and the
   complete specification-to-code audit. No whole-SDK completion claim is made.

## Boundaries that must survive

- Reports are producer claims. Generic checks cannot prove fidelity, opaque-data
  dependency closure or the offered schemas behind a reused assertion.
- A complete pull means exhaustive and no implementation-unsupported omission.
  Loss, conflicts, invalid parts and kind exclusions do not independently negate it.
- Binding proposals may change only data/input/output and carry uses. Known
  author-owned wrapper fields are refused; unknown metadata gains no edit authority.
  Guards are role-local: nested data and whole schema proposals remain opaque.
- No published proposal encodes deletion. Omission means no proposed change;
  data:null sets null; input/output:null is invalid. Removal-only stale drift requires
  truthful operational inability, not fabricated current/unmatched status or a
  private delete sentinel. A caller may make a separate explicit document edit.
- Comparison has its own profile rules: reference replacement, explicit per-side
  roots/bases, opaque unknown keywords, exact values and union multiplicity. It is
  not general schema equivalence. Plain anchors and ambiguous repeated-first-enum
  intersections remain explicit unsupported capabilities.
- Ordinary compiled schema preparation currently has a confirmed allocation gap.
  The same attempt must cover projection, transformations, all backend builds/retries
  and bounded error origins. A source-size multiplier alone is insufficient: repeated
  unevaluated annotation paths grow nonlinearly. Derive a structural bound or route
  unsupported shapes to the bounded semantic path before the backend allocates.
  No existing memory/performance allowance has been waived.

## Local recovery evidence and validation

The current plan snapshot, source-pull design and its 74 controls, and the ordinary
compiler allocation review are also saved under `docs/alignment-checkpoint/` on
this branch. They preserve recovery context; their original evidence-relative
paths refer to the coordinator rather than new SDK dependencies.

The complete source inputs, runnable review probes and receipts remain in the separate local coordinator:
`/Users/matt/Code/ob-pj/openbindings/sdk-migration`.
Read `plan/PLAN.md`, `design/2026-11-02/{architecture.md,api.md,source-pull-implementation.md}`,
and `receipts/2026-11-02-preparation/{source-pull-design-review,comparison-implementation-review,ordinary-compiler-budget-review}`.
The source-pull design receipt contains 74 independent controls. The pinned current
inputs are in `inputs/current-c1419c9-5815fb1`; do not substitute the older inputs.

The SDK checkout is that coordinator's `sdk` directory. `/tmp/ob-sdk-env.sh` selects
the qualified local Rust 1.99 toolchain, offline dependency cache and target directory;
Rust 1.88 is under the SDK's ignored `target/toolchains/1.88.0`. Incremental compilation
is disabled for the local Rust 1.99 compiler. Use repository checks and preserve the
exact pinned dependency versions. Relevant immediate checks are:

```sh
cargo test --locked --offline -p openbindings-source-pull
cargo clippy --locked --offline -p openbindings-source-pull --all-targets -- -D warnings
```

No decision currently requires Matthew. Continue independent unblocked work if one
arises. Do not merge to main, publish packages or deploy without separate authority.
