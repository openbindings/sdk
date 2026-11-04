# Reuse of completed root leaf proofs

The evaluator reuses a completed certified leaf proof through its existing
call-local memo after ordinary alias, scope and cycle resolution. Eligibility
requires a multiply referenced original target and the exact live root occurrence.
The key retains the original target and complete effective context. Direct leaves,
child values and member-name tokens keep their existing dispatch. Only completed,
cut-independent States can be published; errors and provisional cuts are not
cached. Outcome and Explain have separate Runs. No new cache or replacement
schema representation is introduced.

Two flags share the existing preparation descriptor; one occurrence ID belongs
to Run. Independent controls cover nonzero retained-subtree roots, scope/effect
identity, cuts, exact witnesses, concurrency, metadata publication, limits,
cancellation and child-instance exclusion on Rust 1.99 and 1.88. Native Run grows
by 8 bytes; the sampled Wasm layout uses existing padding. Descriptor and Frame
sizes do not grow. The later pure eligibility-check reordering has a separate
static review and measurement. The six selected gate-order comparisons are essentially neutral; no additional speed claim is made.

The first experiment exposed a real 64 KiB reservation regression. The previous
State-copy estimate charged 256 bytes even when both annotation maps were empty.
Empty clones allocate nothing on either supported Rust version. The corrected
policy reserves zero only for two empty maps, preserving work checks, the
ordinary zero-byte reservation check, shared-attempt failure ordering and every
nonempty-map charge. A permanent public test covers satisfying and failing
values in both Outcome and Explain. The exact unchanged original benchmark
request succeeds again; no quota or request was adjusted.

Selected corrected measurements against `f62f477` improve the 5,000-reference
evaluation by about 13% and the repeated expensive-pattern case by about 15 times.
Ordinary selected evaluations improve or change little. Before the separate
gate-order experiment, two scaled child cases are about 3–4% slower. These costs
remain explicit; this checkpoint does not accept a full-campaign residual.
Matched allocator observations for six canonical compile/evaluation cases and
two scaled child cases show no heap or allocation-count increase. The child
cases publish no new leaf entries.

Parent checks pass the evaluator suite, required 1,546 values, lint and separate
release stack/frame controls in their recorded source scopes. The public budget
tests also pass after the pure gate-order edit. Independent reviews and source
hashes distinguish the initial root implementation, reservation correction and
gate ordering; their results are not relabeled as whole-source qualification.

The copied receipts exclude executable binaries and bulk private source trees.
Recorded commands refer to the coordinator layout. Full original/candidate G6
acceptance, final host/package qualification and the complete alignment audit
remain open. This feature-branch checkpoint grants no merge or release authority.
