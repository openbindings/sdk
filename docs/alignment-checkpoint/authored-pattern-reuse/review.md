# Authored-pattern reuse: bounded implementation review

**PASS for the reviewed cache/default-capacity slice**, after the explicit
source-copy work precharge correction. Final production identities are in
`final-source-hashes.json`; the five production files match the independent
copied-source/public-link snapshot. Parent-only probe comment/ceiling changes are
recorded separately. This is not a whole-SDK or G6 performance PASS, nor a restored
dependency-wide CPU/allocator certificate. The reviewer made no SDK edits.

## Implementation assessment

The authored-source index is owned by the existing temporary Programs within one
Construction. Its fixed-size key binds source length/hash and Reader byte/depth
policy. Candidates still compare exact decoded source; hashes are not identities.
Actual Plan occurrence/profile/effect binding stays outside the shared Program.
Source hits skip Reader/normalized construction while checking shared control and
the aggregate budget. Completed local Undefined/Unsupported/Limited states can be
reused; shared failures and partial products cannot become cached successes.

The narrow anchored ASCII-literal subset deliberately bypasses this second index.
It already has a direct Reader path, so copying 380 long literal sources would add
temporary memory without avoiding meaningful expansion. Normalized sharing and
the initial source-extent guard still apply. Independent copied-source tests confirm
the bypass leaves no authored keys.

Key allocation, index growth, hash/candidate comparison and publication/rollback
work are prepaid at their SDK sites. The review found that the newly owned source
copy originally lacked its separate size-proportional work precharge: eligibility
and hash scans could already use both preceding source-sized charges. The parent
added `charge_work(source.len())` immediately before `to_owned`. The allocation
reservation was already present. All independent builds below use that correction.
This is a specific SDK copy-site finding, not an attempt to revive the superseded
dependency-operation ledger.

No RefCell guard crosses Reader, compiler or Control callbacks. A failed source
publication removes its exact appended bucket slot. This matters because distinct
source spellings can share one normalized Program Arc. Only a normalized entry
newly created by this attempt is eligible for normalized rollback; an existing
shared alias is preserved. Fixed-key collision lookup plus exact text equality
keeps different meanings apart. Temporary sources/forms die with Construction;
the Plan retains immutable programs and each evaluation owns search scratch.
The aggregate footprint/reservation charges remain spent after interrupted work.

The explicit default SDK-work allowance is 100,000,000. The 512MiB SDK reservation,
structural compiler profile and representation ceilings remain unchanged. This
implements the approved capacity-policy change; it does not claim that the old 20M
default now suffices, or that 100M is a wall-clock/dependency instruction bound.
Comparison and source-pull's independently scoped policies are unchanged.

## Independent qualification

Both Rust 1.99 and the minimum Rust 1.88 used freshly built actual core/internal
dependencies, with commands/artifact identities saved. The private evaluator source
copy is production-identical except for appended independent tests. No SDK private
bridge or temporary production mutation was used.

- **30 private groups per toolchain:** all 10 production source-cache preparation
  groups plus 4 independently authored groups; all 6 immutable-program seam groups;
  all 10 Construction groups. This includes selected-meta/secondary Plan sharing,
  exact footprint, source-policy changes, forced collisions, concurrent per-call
  caches and nested aggregate ownership.
- The independent late-publication controls cover both an alternate source
  spelling sharing an existing normalized Arc (`[ba]`, 25 observed cancellation
  checkpoints) and a new normalized program (`[ef]`, 63 checkpoints), while two
  healthy colliding siblings remain reusable. They check both source and normalized
  indexes after every interruption. Terminal source hits succeed under zero new
  SDK allocation allowance and keep the same owner; ignored tighter-child failures
  still poison the enclosing attempt even when the next operation is a cache hit.
- **Actual public preparation:** unchanged single 5,000-shorthand, four distinct
  forms and 380 repeated sources all prepare and return S on `{}` at the new actual
  defaults. The fifth distinct form refuses with `pattern-total-byte-limit`.
  Explicit 20M refuses all four cases with `preparation-sdk-work-units`. The positive
  value intentionally leaves the property predicates unused; this is not a claim
  that an out-of-profile 5,000-shorthand predicate matches when demanded.
- **16 public 6,000-shorthand controls per toolchain:** both sibling orders and
  Outcome/Explain preserve exact backtracking/non-word-boundary/size reasons for
  the selected property; the empty object remains S. The direct private original
  control also passes on both versions.
- A warmed original schema containing `[ab]`, `[ba]`, `[ab]` has an independently
  searched exact scoped work boundary of 3737 and reservation boundary of 19896.
  Exact succeeds, one-below refuses, and retry succeeds, on both versions.
- All 170 actually observed public preparation cancellation checkpoints for that
  schema return Cancelled and permit a clean default retry, on both versions.

The public counting-allocator observations are byte-identical between toolchains
after excluding absolute process baseline. Inputs and the warmed environment are
outside measurement; each measured preparation/evaluation owner is dropped before
the end comparison. Every row returns to its exact baseline:

| Job | Incremental peak bytes | Requested bytes | Outcome |
| --- | ---: | ---: | --- |
| Single expanded source |1,332,138|7,278,379|S|
| Four distinct expanded sources |5,208,436|29,097,803|S|
| Fifth distinct form |6,500,270|36,366,944|Typed total-form refusal|
|380 repeated sources |1,336,274|7,330,443|S|

These are scoped observations, not arbitrary-build memory guarantees. The parent's
unchanged original probe also retains its tighter control-relative form-memory
ceiling, literal 380 positive and 100 search-cache job. Parent evidence is copied with
explicit `parent-` names and is not misrepresented as an independent rerun.

Two initial private-driver compile failures came from capturing non-Sync Cell/
RefCell state in a Control closure. Their logs and initial test text are preserved.
The corrected driver uses atomic counters and independently enumerated observed
checkpoints. No production behavior or expected semantic outcome was changed to
repair those harness errors.

## Probe-policy dispositions

The original default expanded-pattern positives are restored under an explicit new
default, rather than rewritten to refusal. The old 20M failure remains a permanent
control. The counted 4-name positive and original 40/300 negative inputs remain.
The approved aggregate structural guard now supplies the latter's typed failure
cause rather than the deleted equal-slot memory allocator.

The physical peak assertion for 40/300 is a **separate explicit policy change**.
Its retired `control + 3*(64MiB/count)` ceiling is not met by the new one-build,
aggregate-profile architecture. Parent observations are about 63.86MB peak, with no
retained completed owner and below the unchanged 2-second refusal gate. The initial
128MiB candidate ceiling was unnecessarily broad. The accepted workload-specific
gate is **64MiB plus the unchanged control peak**, keeping the historical attack,
old ceiling and failed expectations visible. This accepts a larger measured
construction peak for these negative probes; it is not proof of a universal
dependency memory bound, an acceptance of G6 residuals, or a change to the positive
four-name result. Future measurements must satisfy this actual gate.

The separate 100k/1M semantic pattern cases at 64KiB now require S, as independently
adjudicated in the design receipt: preparation owns the program and matching borrows
the text, so the removed per-call compiler/copy charge must not manufacture a Limit.
Their inputs and temporary-peak <128KiB assertions remain unchanged. Oversized-origin
U controls retain their original outcomes.

## Scope remaining outside this receipt

Final fresh Wasm/browser/portable checks belong to the parent and must carry the
final source identity. This review supplies bounded native/minimum-Rust source,
semantic, cancellation and allocation evidence, not those host results. The larger
original-default performance pilot's allocation regressions remain open. No default
capacity or measured-performance success beyond the listed jobs is inferred.
