# Independent Astra review — performance investigation design and instrument

Date: 2026-10-13. Reviewer: GPT-6 Astra, xhigh, independent subagent.
Runtime checkpoint: `33153076564f0997c6a7e364660d16ae7aa0eece`.

Disposition: **no blocking finding before the preregistered diagnostic timing**.
This accepts the bounded diagnostic design and its inspected implementation. It
does not accept the authenticated slice as complete, qualify performance, establish
the cause of historical failures, or authorize another run after a stop condition.
Actual diagnostic outcomes require a separate review.

The review was read-only until saving this record. I ran no build, qualification or
benchmark. I inspected the existing performance policy, checkpoint summary and CPU
run 1/run 2 evidence; the measurement consumer and runner; the current adapter's
`security.rs` and `invocation.rs`; and the new preregistration and diagnostic runner.
Old binding drafts and derivative authorities were excluded. Current private-kind
behavior is experimental implementation policy, not binding-specification authority.

Reviewed instrument hashes, before diagnostic timing:

| File | SHA-256 |
| --- | --- |
| `performance-investigation-plan.md` | `a5ac5bc1692d2d5bc243f0d39d4c666c0db628fadff13ac52e7cd7d478e34f51` |
| `examples/openapi-json-path-consumer/examples/auth_measure.rs` | `265c04b1c3c3f775cce492588181f324c05d4bc61fa956e590ba4e09c8b58677` |
| `tools/investigate-openapi-auth-performance.py` | `2c6da3377deaf479c06eb989e43c96524098d815ae9233748a9c10899d5c9069` |
| `tools/measure-openapi-auth-json.py` | `57efaefdc57546ec9b6a8b010fcaf5c3ef6d509937889f68db20fff1c7d1835e` |

## Independent findings from the existing evidence

Both historical runs remain failed under their registered rule. Run 2's operation
AND/64 KiB rows exceed their ceilings in process pair 0 by approximately 702 ns
(3.0.4) and 293 ns (3.1.2). The relevant individual condition samples vary by several
microseconds. Process-median spread below 15% does not bound the uncertainty of a
difference of four independently selected medians.

The 3.0.4 failing row gives a useful diagnostic clue. Its registered unadjusted
four-median residual is approximately 6,370 ns, exceeding the 5,668 ns fixed-plus-
context allowance. The corresponding seven round residuals are approximately
`[-816, 5408, 2463, -142, 3716, 4564, 3811]` ns, each below that allowance. Therefore
median selection and temporal variation materially affect this calculation. The
3.1.2 row also has individual round residuals exceeding its allowance. Neither
observation proves a timing artifact. Replacing differences of medians with a median
of differences would change the registered statistic and is not an acceptable repair.

The reviewed adapter code exposes no obvious authentication-only traversal of the
64 KiB body. Successful security application handles context, scheme selection,
credentials and origin grants. The lazy choice inventory already avoids constructing
challenge alternatives on success. Adapter origin validation overlaps with the
client's final target checks, but validates every supplied grant and contributes to
refusal semantics. Removing it without preserving those outcomes is not justified
by these measurements. I recommend no product runtime change for this experiment.

The direct reference includes request-body conversion, owned attempt credentials,
request preparation, client origin/header checks, finite response processing, result
conversion and destruction. It supplies owned copies of the fixture's approved
origin. Adapter interpretation of the richer role context is legitimate measured
overhead. The inspected harness does not replace those timed operations with
preprepared requests or move body work outside the call closure.

## Design and implementation verification

The preregistered experiment has four AND/64 KiB rows, three fresh processes and
four configurations per row. Configuration order rotates by row plus process;
across the 12 row/process combinations each configuration occupies each position
three times. Conditions are rebuilt inside each configuration. Null labels use the
same anonymous source and context work but independently construct inputs, source
caches, facades and host/oracle state. This measures instance-placement variation
as well as timing variation, which is appropriate and explicitly disclosed.

Both diagnostic methods perform the same single separate preflight call for each
condition. The oracle checks target, body, exact credential header values and one
dispatch; the adapter source becomes warm. No extra unmeasured call precedes a
balanced block. Existing timed closure work and destruction are preserved. The
historical default mode retains its original per-sample warm-up and is not silently
represented as the standardized diagnostic arm.

The balanced implementation uses the four prescribed Williams orders with blocks
of at least 2 ms. It finishes each full 16-block cycle and continues until every
condition has accumulated at least 200 ms. All seven reported samples use summed
elapsed nanoseconds divided by summed calls. The runner validates the sample
counts, order, complete cycles, positive call counts, minimum durations and
aggregation against retained raw blocks. The larger accumulation is fixed before
timing and satisfies the frozen minimum of 50 ms.

The runner computes condition medians, their direct and adapter differences, and
the unadjusted residual. The diagnostic formula remains
`Ca - C0 <= max(0, Da - D0) + 5000 + 2*S`, with exact encoded context sizes 334 and
332 bytes for the two real-auth fixtures. No null correction or alternative
statistic is used. All three absolute null quantities must be at most 1,000 ns for
every balanced null row/process. The full fixed diagnostic matrix runs even after
an early guard failure. Any guard failure produces `inconclusive-stop`, with no
qualification permitted under this investigation.

If the guard passes, the plan permits at most one complete qualification with all
24 unsecured and 16 incremental-auth rows, three alternating fresh process pairs,
identical measurement source, the closed SDK baseline and unchanged gate formulas.
The 15% baseline-spread rule, per-pair requirement and resource budgets remain in
force. I also inspected the final runner's pre-timing update. Balanced mode requires
an adequate diagnostic record with 12 passing null rows, matching measurement source
and matching frozen qualification-runner hash. It selects the balanced sampler for
both binaries, waits the fixed 10 seconds after builds, retains compressed raw
blocks and validates order, durations, counts and aggregation for all 24 workloads.
The existing formula, source-equality check and spread calculation are preserved.
No blocking finding arose from this tooling update. Actual diagnostic and any final
qualification evidence still require verification before a completion claim.

## Caveats and reporting repairs

The 1,000 ns guard is a prospective engineering resolution target, approximately
18% of the fixed-plus-context allowance. It is not a statistical confidence bound
and cannot by itself resolve the historical 293/702 ns excesses. Component guards
matter because direct and adapter null biases can otherwise cancel. The actual
auth margin must be reported alongside observed null variation; passing a later
gate cannot establish that an earlier failure was caused by noise.

Shorter interleaving changes temporal separation and potentially cache behavior.
The null experiment tests the new instrument under the prescribed conditions; it
does not eliminate every source of uncertainty. Williams predecessor balance is
exact within the four-condition orders. Concatenating orders and cycles adds
boundary transitions, so the complete stream is not perfectly balanced for every
predecessor. Raw order is retained for inspection.

I raised two reporting issues before timing. Both were addressed without changing
the timed source: the runner now emits across-process SDs for direct null deltas,
adapter null deltas and residuals; the preregistration now explicitly limits its
predecessor-balance claim to within-order transitions. Neither issue required a
different sampling schedule or a runtime repair.

This review supports executing the fixed diagnostic once and honoring its result.
An inconclusive stop completes this bounded investigation, not the authenticated
slice. Both original failed runs remain part of the evidence.
