# Inactive actual-HIR admission: independent review

Status: bounded PASS for `nfa/admission.rs` and its fixed profile at checkpoint 34f898c, pinned in `source-hashes.json`. The fold is not yet wired into the builder. The subsequently introduced profile/budget module, aggregate hosting/debits, retained-program publication and final activation/default/performance gates are excluded.

## Meaning and source review

The three root scores match the independently frozen recurrences in `../lean-admission/folds.md`: expanded compiler dispatch; helper input, including UTF-8 sequences; and empty-tail join pressure. The implementation walks original immutable HIR with a scalar completed child and an ancestor vector. No width-sized child metadata tree is retained. Composite dispatch, literal-only trie (at least two children), nullable versus non-nullable zero-minimum repetition, bounded-zero prefix, capture and exact-count paths preserve their distinct rules. Concat starts from the first actual child, not an artificial empty prefix. The actual HIR properties drive nullable repetition.

The common sentinel 1,048,577 is above all three per-program thresholds. Saturating operations preserve the bounded arithmetic algebra, including zero times an excessive child. A child saturation is not itself a refusal. Root scores at or below their accepted thresholds are exact; aggregate successful debits must use those exact root scores, not saturation. Arithmetic with existing arity/slice indices remains bounded by actual HIR storage; ancestry backing arithmetic is bounded by the prior 512 named-path guard.

The tail score applies only to the pinned forward, capture-free compiler construction route and its proven constructor order. It does not extend the earlier empty-chain proof to arbitrary Builder graphs. The +2 root union allowance and prefix +1 are explicit. The preserved reverse-numbered-chain negative remains relevant. Replaying these scores is not a new dependency CPU/allocation prediction.

Only Start, End and WordAscii looks are accepted by this closed route. Other look modes return Representation after observing shared failure. The outer source/grammar admission remains required; these private scores do not certify arbitrary externally manufactured HIR.

## UTF-8 scratch argument

The pinned iterator stores two-u32 ScalarRange entries. A descent can create at most one pending surrogate split, three length-boundary splits and two endpoint splits for each of three base-64 masks. At most eleven pending entries therefore fit capacity sixteen. The conservative cumulative sequence 1+2+4+8+16 is 31 entries, or 248 requested bytes on the reviewed native/Wasm layouts. The 512-byte reservation precedes the first iterator construction and covers this single scratch owner. `reset` clears but never shrinks/replaces its backing. The implementation initializes it lazily once, reuses it across all non-ASCII ranges/classes, never collects sequences, and drops it before final publication. ASCII-only classes allocate no iterator. This is a narrow SDK scratch reservation, not revival of the full dependency-site ledger.

Control work/checks occur before nodes, Unicode ranges, iterator steps and reservations. Ancestor and UTF-8 owners drop on every error. The explicit failed ancestor-reserve branch checks cancellation before returning Size. Final scratch destruction precedes the completion check. No control, scratch or mutable cache is retained in the returned scalar scores.

## Independent evidence

* Rebound source/binary hashes and independently recomposed expectations from the frozen recursive model plus 37 hand controls. Fresh Rust 1.99 and 1.88 replay: all 11,274 triplets match exactly, with no retained scratch and every requested scratch total within the explicit reservation. Native observations match byte for byte. Maximum observed requested bytes 2,216; peak 1,736; reservation 2,720. These are scratch-only finite observations, not NFA-build measurements.
* Additional actual-source controls, built independently against each Rust version: 448 checks pass. They include all 276 ordered endpoint pairs from 23 Unicode cut points, each with requested scratch at most 512 bytes and ASCII allocation zero; every observed cancellation boundary for four representative Unicode/nullable/ancestor shapes with complete cleanup; zero-reservation refusal before Unicode scratch allocation; five unreviewed look refusals without allocation; exact/over-limit repetition and zero-annihilation witnesses; and 80 concurrent reads of one immutable HIR.
* The first extra-control driver declared the shared HIR inside the scoped-thread closure and failed Rust's lifetime check. Moving that test owner outside the scope fixed only the harness. `initial-borrow-harness-error.log` preserves the failure. No production edit or oracle change was made.

No new finding remains in this inactive fold. Activation must still apply upstream Reader expansion and AST/HIR limits, debit all three accepted scores in the one shared preparation construction before backend entry, preserve local-vs-shared failure semantics, then account the final immutable representation before publication. Full adversarial construction/time/stack and frozen workload gates remain explicit.
