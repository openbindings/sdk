# AST parsing and destruction work proposal

Status: source-derived proposal for independent review. This is not approval of
the complete compiler or a production implementation. The arithmetic in the
receipt program is intentionally ordinary Python integer arithmetic; production
arithmetic must be checked and paid before entering the dependency.

## Scope and units

The exact regex-syntax 0.8.11 source hashes are in `source-pins.json`. The actual
SDK `normalized::inspect` guard is run before every measured parse. It admits
ASCII source with flat positive ordered scalar classes, capture/noncapture
groups, the fixed `(?-u:\b)` flag form, assertions and quantifiers. It excludes
named captures, comments, whitespace mode, Unicode property lookup, class set
algebra, nested classes, speculative ASCII classes and word-boundary extensions.
It does not assert that the backend can represent every guarded sequence.

Work units follow the HIR/NFA review: one byte inspected/copied, one target word
of an explicitly transferred value, or one bounded scalar comparison/update.
They are not machine instructions or elapsed time. `W(T)` is
`ceil(size_of(T)/size_of(usize))`, using the separately measured native and Wasm
layouts in `syntax-layout-table.json`. Allocation reservations are independent.
Unused capacity is not counted as initialized elements copied.

Let B be normalized bytes; t be literal plus assertion tokens; g groups; b pipe
tokens; q repetitions; k classes; r class items; f flag items. The guard supplies
these counters before parsing. Let C=g+b+1 and
N=t+k+3g+q+3b+2. C bounds all concatenation contexts and N includes AST payloads,
empty branch roots and temporary empty group roots. These are the conservative
node bounds used by the separately reviewed allocation model, not observed AST
sizes. A fresh parser is required; retained state from a previous parse is outside
this schedule.

## Parsing and source access

`ParserI::parse_with_comments` advances monotonically. Each dispatch either
consumes source or returns an error. The guard excludes every rewind path.
`bump` is called at most B times and examines at most three scalars per call;
each scalar is one source byte here. Main dispatch occurs at most B times.
Flat-class dispatch occurs at most r+2k times, for members and two brackets.

The source-access allowance is `32B+32g+16k+16q+64`. The B term covers cursor
reads and per-byte primitive, span, escape, decimal/hex, and range tests. The
extra group term separately covers prefix probing: the four lookaround prefixes
have total length 10, the two named-capture prefixes length 5, and `?` length 1.
Charging twice their total length covers both `starts_with` and the successful
prefix character iterator, even though almost all probes fail under this guard.
Class and repetition terms cover terminator/range and lazy/operator lookahead.
The constant covers empty input and terminal/error dispatch. The proposal also
charges `128*(B+g+k+q+1)` fixed scalar control steps for the non-copy parser work:
offset/line/column updates, EOF/whitespace-disabled checks, token dispatch,
borrow/cell operations, capture index checks, flags and range validity. These
are separate from the typed payload transfers below. The 128 coefficient is a
conservative source-body allowance requiring independent review, not a bound
inferred from the operation-event measurements.

Hex and decimal scratch receives at most B ASCII digits in total. An insertion
plus geometric relocation allowance is 5B; one u32 radix scan costs at most B.
`clear` retains capacity and has no character destructors. The only flags are
the two items `-u`; flag lookup/addition is bounded by two comparisons per item.

At most one error-owned copy of the complete source is reachable. The generic
parser has paths that replace an earlier decimal error with an unclosed-count
error, but the guard guarantees both decimal syntax and the closing brace. An
overflowing first decimal is propagated before parsing a second one. Other
parser errors return immediately; a nesting-limit error arises only after a
successful parse. The schedule reserves B copied bytes for that error path.

## Typed transfers and growth

The exact per-site terms are executable in `summarize.py`. Payload transfers are
charged separately for Literal/Assertion (Literal is at least as large), Group,
Repetition, ClassBracketed, Concat, Alternation, Span and FlagsItem. A factor of
six covers local construction, result/enum transfer and boxed installation at
these fixed source paths. Concat argument/return transfers are separately charged
for group opening/closing, pipe and repetition helpers.

A vector with p total inserted initialized elements has fewer than 4p elements
relocated over geometric growth. Initial minimum capacity contributes no copy;
pops do not shrink capacity. Charging 6p therefore covers insertion, removal and
all relocations. Sum this over all vectors, retaining the element's actual width.
This applies to parser GroupState and ClassState stacks and AST child vectors.
Flat class items additionally cross Primitive/Range/ClassSetItem conversions;
their term is 10r*W(ClassSetItem). ClassSet replacements and temporary class roots
are separate. No operation clones a child subtree. `into_ast` either constructs
a new wrapper, returns its one child, or creates an empty leaf.

## Nesting visitor and cleanup

The heap visitor reaches every AST node at most once before and after its
children, and every class item/union at most once before and after. Its frame
push/pop sequence follows child edges; tail slices advance once per edge. N AST
nodes and r+k class nodes cover this work. Charge 32(N+r+k) scalar dispatch/depth
steps: at most 16 for enter/descend and at most 16 for ascend/continue, including
`induct`, `pop`, `child`, continuation tests and NestLimiter depth checks. The
initial proposal's 12 coefficient omitted an explicit allowance for these visitor
helpers; independent review identified this gap. The correction does not borrow
slack from parsing. Separately charge
6[N*W(AstVisitFrame)+(r+k)*W(ClassVisitFrame)] for frames and relocation.
Early refusal only reduces the traversal. This accounts for the parser's own
NestLimiter; subsequent SDK metadata traversal is a separate phase.

AST destruction is iterative. Taking a group/repetition child replaces it with
an empty leaf; draining a concat/alternation transfers its owned children. Each
non-temporary AST enters a destructor work stack at most once. A popped wrapper
subsequently drops with empty children, so it does not traverse them again.
Temporary empty roots plus those replacements are included in
D=2N+2g+q+C destructor invocations. Charge 8(D+N) dispatch steps, 6N*W(Ast) stack
transfers/relocations, and 6(N+g+q+C+b)*W(Span) empty payload initialization/transfer.

Flat ClassSet destruction similarly drains each union item once. There are no
nested brackets or binary children. At most r+k items are popped, and at most
r+4k destructor invocations include temporary empty roots and final empty
wrappers. Charge 8(2r+5k) dispatch steps and 6(r+k)*W(ClassSet) transfers,
relocation and empty replacement. These allowances are reserved before parsing,
including for parse failure and successful AST destruction after later phases.

## Evidence and remaining gate

The 4,331 input rows include the unchanged Reader corpus, independently authored
guard controls, eight long-input families through 65,536 units, nesting-limit
refusals and 3,000 deterministic generated trees. Forty-four source rows are not
Reader Linear forms and are excluded before backend entry. All 4,287 remaining
rows pass the real guard: 3,355 parse successfully and 932 return representation
or nesting refusals. Deep raw-form controls exercise AST traversal/destruction;
they do not assert a production construction depth floor.

`instrumentation.json` names every inserted counter. The observations separately
record character access, cursor/prefix/radix scans, main/class dispatch, visitor
pre/post events, destructor invocations and destructor work-stack pops. Every
observed event is within its named prospective bound. `summary.json` records
hashes and results. This instrumentation does not independently count every typed
move or scalar control step: those remain source arguments for review. It also
does not qualify physical stack, HIR/NFA work, SDK metadata or an aggregate Plan.

The allocator replay uses the real guard inventory for the prospective allocation
site model. Development Rust and Rust 1.88 each replay all 4,287 cases with no
undercharge or retained allocation; their parse results, event counters and
allocation requests agree. The actual 32-bit Wasm binary, with a 128 KiB linear
stack and normal wasm-bindgen imports under Node 22.23, also passes all 4,287
guarded cases with identical parse/event results, no allocation undercharge and
no retained allocation. It requests 109,358,439 bytes against 595,575,550 reserved
across this deliberately large corpus. Native requests 210,703,651 against
1,162,497,518 reserved. These are cumulative requested-byte totals, not peak
footprints or default-job limits. The native/Wasm site formulas use separate
private layout widths. Commands, binary/source hashes and observations are in
`allocation-summary.json`, `msrv-summary.json` and `wasm-summary.json`.

The initial standalone Wasm build lacked the Cargo package name required by the
binding macro; the first runner then used the Rust spelling of a JS-reserved
export instead of the generated `_case` spelling. Both harness failures are
preserved. Neither entered the AST probe or required an SDK behavior change.

Before production adoption: independently review every coefficient and reachable
error path, implement checked arithmetic, replay target layouts and allocator
controls against the implementation, test one-below/exact/cancel/overflow cleanup,
and combine with the other construction phases and unchanged consumer jobs.
