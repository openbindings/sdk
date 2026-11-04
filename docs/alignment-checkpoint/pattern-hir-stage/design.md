# Owned HIR translation stage

Status: implemented as an inactive private stage; parent native/host gates and
independent implementation/resource review pass. The guarded AST and checked
streaming inventory have independent approval. NFA construction and whole
compiler activation remain open. No existing API or
stack_bytes convention constrains this design.

## Boundary and ownership

Use one private construction function receiving the exact normalized borrowed
form and enclosing Control. Its internal normalized/AST representation limits
are fixed at 33/250; lower authored limits have already applied in Reader. An internal
logical construction policy supplies non-bypassable prefix/equality ceilings. It executes guard/AST construction, consumes
that Parsed owner through inventory, checks prospective logical HIR admission, prepays
translation work/allocation and invokes one fresh translator. Keeping the exact
form and its AST together within this function avoids exposing a second form
argument alongside a reusable certificate.

The authored Reader group ceiling is 32 and can be made stricter. It must be
enforced before Reader::group_body, not retroactively by HIR admission. The
normalized grammar ceiling is the checked authored allowance plus one: the
Reader only adds atom-local fixed boundary and NOTHING groups. Authored groups
and supported terminal lookahead preserve their one group. Keep these two depth
meanings distinct; a 32-deep source with an inner boundary must not fail merely
because its normalized spelling has 33 groups. The private AST representation
ceiling is separate from both source-group and translation recursion limits.

Explicitly select TranslatorBuilder UTF-8 mode true, matching the present ordinary
PikeVM factory, and otherwise the reviewed closed-grammar defaults. The fixed
ASCII-boundary group remains local to that group. The earlier review tools that
used utf8(false) are evidence only where their admitted scalar forms are proved
equivalent; they do not choose the production configuration.

A successful result owns the immutable HIR and no AST, translator, source copy,
caller control or mutable cache. Destroy translator ownership and AST ownership
before the completion check. If the check fails, drop the HIR using its prepaid
cleanup. On translator failure, drop its diagnostic without formatting, retain no
backend error/source payload, and return a local representation refusal. Shared
attempt cancellation/exhaustion remains sticky and aborts the enclosing attempt.
Later NFA retries borrow this single immutable HIR; they do not parse or translate
again. A local profile/representation refusal is never an Undefined pattern effect.

## Reservations and profile admission

The inventory supplies checked, separately named HIR work/allocation site sums.
Charge these before building the translator. Include the fixed entry/admission/
cleanup-owner transfers and any reachable source-owning error path explicitly;
independent source review must decide whether that error path is excluded by the
closed grammar or needs a separate borrowed-form-length reservation. Do not assume
an unobserved error cannot allocate. No diagnostic formatting is part of this API.

Before translation, use the prospective maximum simultaneous prefix calls and
recursive equality height, not a finished HIR observation or a byte-length guess.
The independently reviewed internal HIR ceilings are 64 prefix-constructor calls
and 256 equality levels, with frozen boundary controls on both sides. A caller's larger source/group setting cannot bypass them. A
failed logical admission is a typed local construction-limit refusal. Backend AST
nesting remains a representation limit, not a physical-stack measurement.

The Rust source SDK promises these checked logical limits. Rust>=1.88 is build
compatibility, not identical emitted frame sizes or a byte-exact physical-stack
promise. Native/debug/LTO/source-build users provision adequate host stack; actual
available-stack floors are qualified and reported for recorded builds. Shipped
Wasm artifacts can carry an exact linked stack profile. There is no runtime Rust
version allowlist and no public stack_bytes-to-depth promise. Independent review
confirms that the pinned specification does not require either restriction.

This replaces the earlier tentative universal 256 KiB construction-entry proposal
as a public source-SDK contract. Its source recursion arguments, measured frames,
128/256 KiB observations and negative controls remain valuable unchanged evidence.
They cannot be advertised as guarantees for arbitrary Rust compilers. The simpler
contract does not waive required native/default workloads, measured stack/performance
or shipped-host qualification. See [logical construction limits](construction-limits.md).

## Validation sequence

1. Independently review configuration, profile boundary, complete error ownership
   and exact charging order. Freeze controls before implementation.
2. Implement the inactive private ownership stage. Test exact/one-below work and
   allocation, every cancellation boundary, no translator entry after failed
   precharge/profile checks, clean retries and preserved first failure.
3. Compare actual HIR structure/properties and resulting pinned capture-free VM
   behavior on unchanged retained/independent/generated admitted inputs. Keep
   backend-profile refusals separate from expected-value successes.
4. Measure actual cumulative requested allocation and cleanup on current/minimum
   Rust and actual Wasm/hosts, including failure and late cancellation. Source
   instrumentation and the existing phase proofs supplement finite controls.
5. Finish target/build stack admission and combined successful/failed-retry
   qualification before production adoption. Then implement the HIR-derived NFA
   inventory, fresh geometrically bounded NFA attempts, immutable retained program
   ownership and call-local caches. Replay complete default jobs before switching
   the original evaluator into the ordinary path.

This is an implementation mechanism for the adopted schema evaluator, not an
OpenBindings document-model concept or an obligation to expose HIR/AST/profile
objects in the public SDK. Public terminology continues to follow the pinned spec
and interfaces.
