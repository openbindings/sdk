# Explicit frames for original schema evaluation

Draft for independent review, not an implementation approval. This expands the
reviewed original-evaluator sequence's execution step. Existing internal methods
are inspection points, not API or organizational constraints.

## Meaning and boundary

One evaluation interprets the prepared original schema graph over an exact input
value. A frame represents an unfinished application of one schema at one original
instance occurrence and effective dynamic scope. It is private execution state,
not a second schema model or public evaluator API. JSON Schema applicators own the
combination rules; OpenBindings keeps Satisfies, Fails, Undefined, and actual
inability distinct. No retrieval or kind interpretation enters this engine.

Replace recursive calls between schema applications with an explicit bounded
continuation loop. Keep the current exact predicates, original reference outcomes,
correlated effects, annotation rules and completed-state certificates. The first
increment is a change of execution mechanism; separately review any change to
which branches are demanded. In particular, do not silently interpret a locally
exhausted child as a Boolean, Undefined atom or successful ignored branch.

## Private organization

The public prepared-validator seam remains small. Private execution is organized
by schema meaning: entry/return and completed-state bookkeeping; references and
dynamic scope; logical applicators; object applicators; array applicators; exact
predicates/patterns; and bounded conclusions/witnesses. A scheduling module owns
the work stack and its allocation policy. It does not become an alternative
reference resolver, keyword registry or schema ownership layer.

A frame needs the original schema index, exact input occurrence, effective scope,
its partial State, and the continuation describing the next schema operation.
Continuations express real folds: reference result, all/any/one branch accumulation,
not, conditional selection, object member/pattern selection, item application,
contains counting, and unevaluated-property/item processing. Use small enums and
indices rather than boxed closures or one heap allocation per continuation.
State retains concrete formulas and only demanded annotations, as already
qualified. Ordinary certified leaves finish directly without a retained frame,
Visit or memo entry for each scalar container item.

Original schema data is read from the Plan-owned inventory. Instance cursors borrow
the caller's retained value only for that call. Do not materialize every object
member merely to work around an unnamed Rust iterator type. A private indexed
member cursor/accessor over the existing arena is a justified Rust implementation
mechanism: it must be constant-time, preserve duplicate/exact UTF-16 name policy,
and avoid repeated nth-element scans. Arrays already support indexed access.

Scope is immutable while a frame is suspended. Choose owned/shared scope storage
only after accounting for its actual copies and lifetimes; it must preserve the
current first-binding/dynamic-shadowing semantics and original source identity.
No preparation control, caller cancellation handle or instance can escape into a
prepared artifact or cross-call cache.

## Entry and return protocol

Entry checks cancellation and work, applies the prepared original profile, and
handles a certified leaf directly. Otherwise it selects aliases/references in
original scope and establishes a Visit keyed by schema, exact instance occurrence
and effective context. Complete memo entries may be reused. An active repeat
invokes the existing recursive-cut logic; a provisional repeat cannot reuse an
unfinished State. Preserve the distinction between active, complete, provisional,
opaque and independently certified coordinate facts.

Each suspended parent requests one child. The child completes into a return slot;
its continuation combines that result according to the applicator, then resumes
or requests the next child. Failed schemas' annotations are guarded by the final
validity before completed-state memoization. Complete facts and provisional facts
remain distinct. A retry clears transient execution frames and active membership
while retaining only the same already reviewed between-attempt certificates. It
uses the same cumulative Graph and preparation allowance; retry never resets work.

Keep an ordered active path for recursive-component handling. Add a bounded
membership index if needed to avoid a quadratic active-path search on long chains.
Alias-cycle detection has the same issue: a transparent chain must not repeatedly
scan or copy its entire prior path. Any index must use the complete original Visit
identity, and account for hashing/copying scope text. It must not identify schemas
solely by URI, decoded text, or numerical node index from another environment.

## Resource policy

Frames use the existing per-evaluation program/allocation allowance. Reserve the
actual vector capacity and all owned contents before growth; reuse capacity after
popping. Charging a full heap frame for every visited leaf would reintroduce the
large-container regression already removed. No unbounded work queue, cloned
subtree or hidden fresh allowance is permitted. Cancellation and failed reserve
leave no state usable by a later call.

Removing Rust recursion does not make the machine-stack limit meaningless. Audit
the fixed entry/dispatch/predicate call depth and the remaining symbolic Graph
operations separately. Symbolic reasoning may retain its existing bounded depth
until qualified otherwise. Do not replace the old semantic depth32 heuristic with
an unexplained new schema/value-depth cap, or treat stack_bytes as free extra heap.
The measured fixed stack floor, heap frame growth and work charges must explain
both successful default deep/long jobs and honest low-limit refusals.

Original diagnostic witnesses are the subsequent increment. The frame protocol
must preserve the exact schema/input occurrence needed to record a witness, but
should not retain a second pointer tree or format messages in Outcome mode.
Explain's eventual first established problem follows original schema meaning;
old compiler message text/order has no compatibility protection.

## Acceptance and sequence

1. Independently review frame ownership, continuation coverage, exact scope
   identity, active/completed/provisional transitions, allocation growth and fixed
   stack accounting before replacing production execution.
2. Implement the smallest complete execution loop across all applicator families;
   do not advertise an iterative path that calls the old recursive schema method
   for difficult cases. Keep predicate helpers and effect/certificate mechanisms
   where their meaning and bounds remain valid.
3. Force the original engine for all 1,546 required values and all accepted
   recursive, annotation, external/custom-meta and exact-value controls. Add deep
   mixed object/array/applicator cases, reference diamonds and distinct-scope same-
   node visits, provisional retries, cancellation at every observed boundary,
   complete/incomplete memo distinction, low frame budgets and concurrent reuse.
4. Restore the frozen 20k/20,001/50k container jobs, depth and long-reference jobs
   at unchanged defaults. Measure requested/peak allocations and small-thread-
   stack behavior separately from timings. This does not close matched performance
   or approve an allocation coefficient from a finite probe alone.
5. Qualify native current/minimum Rust, actual Wasm/Node/browser and packaged
   consumers. Review the complete implementation before the later default switch,
   diagnostic-witness work and outgoing ordinary compiler removal.

Open design details for review: concrete frame layout and capacity reservation;
private object cursor API; scope-copy/share policy; active-path and alias indexes;
fixed stack floor and separation from symbolic reasoning depth. These are SDK
engineering choices, not decisions requiring Matthew or reasons to preserve old
APIs. Pattern VM preparation and cross-phase syntax facts remain separate work.

## Concrete continuation proposal

Use one private `execution` module with applicator-specific `logical`, `object`
and `array` children. It owns `Frame { visit, value, state, phase }`, where `value`
borrows the call's exact JSON occurrence, `visit` contains the original schema
index and effective scope, and `phase` identifies the next operation and any
partial fold. Read each schema from the Plan rather than retaining a duplicate
schema representation in a frame. Object stages use the existing constant-time
`backend::member_at`; array and branch stages use indexed elements. A frame never
owns all instance members or a queue of all children.

The scheduler first enters one request. A certified leaf completes immediately;
otherwise an entered frame advances until it requests a child or completes.
Suspended parents stay in one vector. A single return slot transfers ownership of
the child's State to its parent's applicator continuation. No closure, future,
boxed continuation or recursive schema fallback is needed. References resolve to
a prepared target request or a completed local effect. Draft-07 reference objects
complete with that reference result without applying ignored sibling keywords.

Logical continuations retain a branch cursor and their conjunction/disjunction/
exactly-one fold, or the condition and selected-arm formulas needed by `if`.
Object continuations retain one member cursor, an optional demanded name, its
selection formula and one pattern cursor. Array continuations retain one item
cursor and, only during `contains`, its existing scalar/formula count state.
Unevaluated stages read the enclosing accumulated annotations. Each family's
return handler explicitly guards or discards child annotations as its meaning
requires; a generic merge-all helper is not its interpretation.

Proposed scope storage is a shared immutable sorted effective binding vector.
Equality and hashing inspect its content, never its allocation address. `enter`
can share its incoming scope when no new outermost binding is visible; a changed
map allocates under the same ledger. This proposal avoids requiring a second
interning authority. Hash/content scans are charged separately from allocation,
and keys preserve ambiguous binding values. A scope interner is an alternative
only if the independent ownership review shows a need for it.

The ordered active path is paired with a content-keyed position index. Alias
traversal uses the same kind of membership index rather than scanning every prior
entry. The active path is removed on completion and cleared on `AbortAttempt` or
terminal error. Only the already-reviewed between-attempt certificates survive a
retry; ordinary terminal limits and cancellation are never retry signals.

Reserve frame-vector growth before allocation, accounting for the requested new
capacity (not only live frame count), checked size arithmetic and allocation
failure. Reuse that capacity when children return. Heap growth is charged to the
program ledger; machine-stack accounting remains a distinct fixed scheduler/
predicate floor plus the bounded symbolic Graph depth. Its exact floor still
requires measured native/Wasm evidence before claiming the execution gate closed.
