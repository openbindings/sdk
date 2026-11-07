# Lean actual-HIR admission folds

Status: independent source review and frozen control model, before production
implementation. This replaces the proposed NFA source-word/site-ledger inventory.
It is a structural admission design, not a universal operation or allocation
certificate. The separate named compiler-path ceiling remains512.

Scope is the exact owned HIR produced by the guarded factory, locked
regex-syntax0.8.11/regex-automata0.4.18, one pattern, forward, UTF-8 enabled,
WhichCaptures::None, existing unanchored-prefix policy. Unknown configurations
must not silently inherit the certificate. No raw public HIR input is introduced.

## Small record and arithmetic

One streaming ancestor vector is sufficient. A completed subtree carries
`dispatch`, `payload`, and a Tail record `{prefix, suffix, maximum, pure, unions}`.
The root returns three scores; Tail is temporary. No per-child vector, site array,
width-sized metadata tree, retained HIR clone, compiler retry table or native
layout substitution is needed.

Use a common finite sentinel M=1,048,577 (one above the largest per-program
threshold). `a ⊕ b = min(M,a+b)` and `a ⊗ b = min(M,a*b)`.
Compute multiplication by division-before-product or checked arithmetic; zero
annihilates even a sentinel: `0 ⊗ M = 0`. These are intentional finite structural
scores, not overflow errors or exact huge-count claims. Do not fail a child merely
because its score is the sentinel: a zero-copy parent can eliminate it. Compare
only the completed root to each limit. Shared Control failures still propagate
immediately and keep their first cause. Vector extent arithmetic remains genuinely
checked allocation arithmetic; it is not replaced with these saturating scores.

If future aggregate limits exceed M, use a separate checked aggregate sum after
per-program admission. Never use the saturated value as if it were an exact
successful debit. All accepted per-program results below the limit are exact
values of the defined conservative recurrences.

## Dispatch score D

Count calls of Compiler::c on authored HIR nodes; this is a structural count,
not all helper calls. Leaves have D=1. Capture is `1+Dchild`. Concat and general
alternation are `1+ΣDchild`. An alternation with **at least two children, all
Literal**, is a single trie helper and has D=1; its children are not passed to c.
A one-literal or mixed alternation must not use that shortcut.

For repetition use `D=1+k*Dchild`, where k=max for finite repetition and
k=max(min,1) for unbounded repetition. These are the actual sequential child-copy
counts in c_exactly/c_bounded/c_at_least. Bounded minimum zero still runs the
zero-prefix helper but does not call c on a child for that prefix; its fixed
helper path is covered by the separate named-path guard. Return `1+Droot` to
conservatively include the possible generated AnyByte child of the unanchored
prefix. Its absence for anchored patterns only makes this upper bound looser.

Proposed limit:65,536/program;1,048,576 aggregate across actual constructions.

## Helper-input payload H

Payload is a sum of named input elements, not bytes of total memory or a CPU
counter. It retains the broad helper inputs that a final state-size ceiling cannot
see soon enough:

- Empty and ordinary Start/End look assertions:0.
- Literal:B UTF-8 bytes.
- Byte class or all-ASCII Unicode class:R ranges.
- Non-ASCII Unicode class:R+Q+L, where Q is the number of sequences yielded by
  Utf8Sequences for all ranges and L is the sum of their byte-range lengths
  (1..4 each). This is exact enumeration on the already bounded class, not a
  spelling-length guess or an always-charged Unicode table.
- The normalized ASCII word-boundary Look:256, representing the fixed byte-table
  breadth. Other word/look modes are outside the factory's current closed grammar;
  do not silently assign them zero if the factory is later expanded.
- Capture:Hchild. Concat and general alternation:arity+ΣHchild.
- Literal-only trie alternation:arity+sum of its literal byte lengths.
- Repetition:k*Hchild with the same k as D.

Return `1+Hroot` for the possible generated AnyByte class input. The fixed cold
Unicode helper table is a separately qualified fixed dependency cost when a
non-ASCII class is reached; it is neither multiplied into this score as an
invented per-byte coefficient nor forgotten in peak/allocation probes.

**Inventory scratch:** Utf8Sequences::new itself owns a small Vec. Reuse one lazy
iterator and its reset method across ranges, with a bounded scoped SDK scratch
reservation and cooperative checks, instead of allocating one per range. The
iterator is not a width-sized class collection. The closed scalar range algorithm
splits at Unicode length/surrogate boundaries and base64 endpoints, so its pending
stack is bounded independently of the size of the character set. The prior
source/layout proof and actual allocator probe can be retained for this small
scratch owner; no full compiler-site ledger is needed. Do not claim the fold is
allocation-free if it uses this iterator or the ancestor vector.

Proposed limit:1,048,576/program;8,388,608 aggregate.

## Empty-tail pressure J

This score keeps the observed compaction-order hazard explicit. A Tail records:
P maximum goto prefix, T maximum goto suffix, C maximum consecutive goto chain,
A whether the whole fragment is goto-only, U an upper bound on union alternatives.
"Goto" is Builder Empty or a singleton Union/UnionReverse; a Look, ByteRange,
Sparse or multiway union stops the chain. Captures are absent from the NFA.

Base records `(P,T,C,A,U)`:

- Empty (and defensive empty Literal):`(1,1,1,true,0)`.
- Nonempty Literal and Look:`(0,0,0,false,0)`.
- Byte/Unicode class:`(0,1,1,false,0)` (its sparse/range predecessors are not goto).
- Capture:the child record.
- Literal-only trie, arity a and total literal bytes b:
  `(1,1,2,false,b+2a)`. In literal_trie::compile every inter-trie-state link crosses
  a byte/range/sparse transition; union children are chunk states or the common
  empty end, not arbitrarily chained trie unions. Leaf transitions bypass a child
  frame. This supplies the short chain bound without a b-squared overestimate.

Concatenation a then b is:
`P=a.P+(a.A?b.P:0)`, `T=b.T+(b.A?a.T:0)`,
`C=max(a.C,b.C,a.T+b.P)`, `A=a.A&&b.A`, `U=a.U+b.U`.
Use an explicit first-child flag: do not introduce a fictitious Empty before a
nonempty concatenation. An empty iterator has the Empty record.

For Times(x,n), zero returns Empty, one returns x. For n>=2:
if x.A, `P=n*x.P`, `T=n*x.T`,
`C=max(x.C,x.T+x.P,n*x.P)`, A=true;
otherwise P=x.P,T=x.T,C=max(x.C,x.T+x.P),A=false.
In both cases U=n*x.U.

General alternate: P=0,T=max(1,max(child.T+1)),
C=max(1,max(child.C),max(child.T+1)),A=false,
U=arity+Σchild.U. This accounts for branch-end→common-empty joins.

Repetition:

- exact n:Times(child,n);
- optional0..1:P=0,T=child.T+1,C=max(child.C,child.T+1),A=false,U=child.U+2;
- bounded min<max:let q=Times(child,min); P=q.P,T=child.T+1,
  C=max(q.C,child.C,child.T+1),A=false,
  U=max*child.U+2*(max-min);
- unbounded min0 with a provably nonempty child: P=T=0,C=child.C,A=false,
  U=child.U+2;
- unbounded min0 nullable/unknown minimum length: P=0,T=1,
  C=max(child.C,1),A=false,U=child.U+4;
- unbounded min>=1:Times(child,min), then T=0,A=false,U+=2.

All arithmetic above uses ⊕/⊗. Return
`J=(Uroot+2)*max(Croot,Proot+1)`.
The +2 permits the generated unanchored two-way union; +1 permits the anchored
empty prefix. Single-pattern assembly and capture-free wrapping add no additional
union. This is deliberately independent of actual state count or pointer width.

The preserved constructor-aware source argument deletes at most U goto edges to
leave increasing-state-ID paths; repeated marked suffixes are then bounded by
U*C. This is **not** a claim for arbitrary Builder NFAs: a reverse-numbered pure
chain disproves it. The source proof and negative remain in
`prepared-pattern-program-review/design/empty-chain-order-proof.md`.
Rechecking its5,019 actual compiler observations with root+2 instead of the old
root+4 gives zero union, chain or two-walk bound failures. This supports the score;
it is not a new full dependency CPU quota.

Proposed limit:1,048,576/program;8,388,608 aggregate.

## Frozen controls and calibration

`controls.json` freezes37 independently hand-counted expected triplets before
production folds exist. It covers leaves/classes/UTF-8 lengths, canonical
literal/class collapse, general versus trie alternation, capture, concatenation,
nullable star, optional/exact/bounded/unbounded copies, minimum-zero prefix,
zero annihilation of huge repetition, all three limit boundaries and typed
synthetic-source scope. The small independent recursive model uses actual pinned
HIR canonicalization but computes the recurrences separately from any new
production fold. Every hand expectation matches. Boundary synthetic HIR controls
do not claim admission by the upstream source profile.

The independent model also ran all1,142 frozen Linear forms,18 independent HIR
forms,10,065 generated forms and12 unchanged performance patterns. No parse
refusals or proposed score-limit violations. Thus these NFA scores do not conceal
or repair the two large AST/HIR attacks: both still fail the earlier profile,
whose original inputs/measurements are preserved. The performance maxima are
D7,H8,193,J18,436; frozen maxima D20,066,H40,065,J20,066 include those two attacks.
These are source/profile calibration results, not new performance measurements.

Before activation add actual production comparisons against every frozen triplet
and all fields of the full calibration; path512 controls remain separate. Check
capped addition/multiplication with zero and near usize limits on32/64-bit targets,
per-program versus aggregate limits, cancellation during range enumeration and
ancestor growth, no backend entry after refusal, cleanup and concurrency. Preserve
required S/F/U and the actual transient/stack/time gates from the resource-policy
receipt. No larger public compatibility requirement motivates these private fields.
