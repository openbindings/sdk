# Bounded source-cache and capacity-policy disposition

The parent accepted this design direction after the open positive-capacity finding
in `pattern-workload-adjudication.md`. This receipt approves the bounded design
for implementation and qualification, **not** unseen code or a completed capacity
gate. No production changes were made by the reviewer.

## Chosen direction

Add a temporary exact authored-source cache to the existing Construction-owned
Programs owner, before Reader expansion. Keep the normalized-form index so
different spellings can still share one immutable Program. This removes actual
repeated expansion/work, including the repeated 380-source job, rather than
lowering an unexplained accounting multiplier. Do not add cross-attempt reuse or
a second independent construction session.

Use an explicit default `max_sdk_work_units` of 100,000,000 rather than 20,000,000,
subject to the frozen job and boundary qualification below. The old number is an
unpublished SDK policy, not a pinned core/interface requirement. Four individually
permitted expanded forms consume roughly 80.5M in the combined Reader/hash/guard
SDK schedule before other preparation overhead; the proposed100M policy has a
concrete capacity purpose. It is not evidence of arbitrary dependency-operation
time or total allocator bounds. Preserve the512MiB SDK reservation default,
compiler structural profile, per-program/aggregate representation limits,
first-cause behavior, and explicit caller-selected lower work limits. Document the
default policy change rather than presenting the old20M job as restored.

No narrower Reader coefficient adjustment is recommended merely to fit the job.
If the actual100M/default-reservation replay fails, preserve that evidence and
revisit the design; this receipt does not authorize successive unexamined quota
increases.

## Ownership, meaning and publication requirements

1. The cache belongs to the same immutable Construction and fixed Reader policy.
   The key must bind exact decoded pattern source and applicable source/depth
   policy, or the owner must make policy mismatch impossible. Environment/original
   occurrence binding remains in the Plan, not the shared Program key.
2. Hashes are only indexes. Every candidate collision compares exact source under
   charged work/cancellation checks. Equivalent JSON escape spellings may share
   after decoding; different pattern source strings must not share solely by hash.
3. Charge actual source scans/hash, comparison candidates, owned keys, bucket/tree
   growth and Arc ownership before their operations. A hit legitimately omits
   Reader/compiler work and structural debits that no longer execute. A source
   cache must not silently retain uncharged keys outside the existing finite SDK
   reservation policy.
4. Publish only completed deterministic local results. Undefined, Unsupported or
   local Limited results can be reused under identical interpretation/policy;
   incomplete Reader state, shared Attempt failure, cancellation or a partial
   compiler product cannot be cached. Observe sticky aggregate/control state before
   reuse and after publication. No RefCell guard may span Reader/compiler calls
   or arbitrary Control callbacks.
5. Multiple authored spellings can share one normalized Program Arc. Rollback of
   one interrupted source-cache publication must identify that exact source entry;
   deleting by Program Arc identity alone can erase a healthy sibling. Preserve
   all colliding and equivalent healthy entries. Failed aggregate charges remain
   spent as already specified.
6. Temporary source/normalized indexes and borrowed controls die with Construction.
   Plans retain only immutable Program Arcs bound to actual occurrences; value calls
   keep their own search caches. No per-value compilation or new global state is
   introduced.

## Required finite controls

- Preserve unchanged single5,000-`\\S`, within-four and repeated380 jobs with
  successful preparation/S on `{}` at actual new defaults. Preserve the fifth
  distinct form/380distinct/five-name total-extent refusal lanes and measured
  memory ceilings. Counted4 remains S; counted40/300 retain typed aggregate
  structural refusal. Keep initial failing default observations.
- Explicit20M retains its earlier work refusal; exact/one-below observed scoped
  work and reservation boundaries remain sticky and retry cleanly. Do not infer
  a numeric boundary from a successful aggregate observation alone.
- Equal decoded source, alternate JSON escape spelling, different authored strings
  with the same normalized form, and intentionally forced hash collisions.
  Confirm source hits skip actual Reader entry and normalized hits skip compilation;
  confirm distinct source/profile policy never gets an incorrect result.
- Cancel at every observed lookup/publication boundary. Retry from clean state;
  check rollback with a healthy colliding sibling and with two source entries
  sharing one normalized Arc. Ignored nested failures and prior failure precedence
  must still prevent successful outer publication.
- Local invalid/unsupported/source/depth/representation outcomes, lower limits,
  reached versus unused operands, and the exact6,000-shorthand sibling reasons.
  No local limit may become Undefined or an unconditionally successful predicate.
- Shared selected-meta/secondary Plan construction, original occurrence locations,
  cache drop/retained-owner observations and concurrent value-call cache isolation.
  Existing required semantic and actual host gates remain in scope for the final
  checkpoint; this is not a new dependency-wide source-word certificate.

## Separate small-proof-budget probe

`tools/diagnostic-memory-probe/src/semantic.rs`'s pattern-limited case uses pattern
`a` on100k/1M scalar text, plus an independently deciding masked-reference branch.
Its semantic result is S. The old64KiB Limit expectation arose from per-value
compiler/text-copy reservations. Once-prepared immutable programs and borrowed
input remove those operations; no current requirement demands exhaustion at64KiB.
It is sound to require S for both unchanged inputs while preserving the measured
temporary peak below128KiB and cleanup assertions. Keep the earlier failing
quota expectation as history. A separate real low-resource refusal may be tested,
but do not invent an arbitrary new cutoff just to recreate the old limitation.
This disposition does not waive the separate expanded-pattern preparation positives.
