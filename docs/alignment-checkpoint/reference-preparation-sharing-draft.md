# Repeated reference preparation: proposed next increment

Draft for independent design review. The frozen OE-05 jobs are 1,000/5,000 sibling
references repeating 256/1,024-byte spellings. They are not deep execution chains.
Explicit frames restore the separate depth gap; these two jobs still exceed the
unchanged preparation work allowance. No limit increase or expected-result change
is proposed.

## Two distinct reads

The current fixed standard meta-schema check has format assertion disabled. Its
`$ref` constraints in the pinned supported editions inspect the JSON string kind,
not URI spelling. The current local syntax reservation nevertheless retains and
multiplies the entire reference token by 64. Separately, `check_from` repeatedly
resolves the same URI in the same original environment and reserves 16 times its
base/spelling size for each referring position. Removing only one repetition does
not establish the complete OE-05 gate.

First audit the exact pinned meta graphs, the evaluator's fixed compilation
options and the actual backend type validators. If they establish type-only reads,
allow the syntax extent fold to replace just a qualifying string token at the
identified reference keyword by a unit header. Wrong JSON kinds retain their
extent and still fail the actual complete original validator. The validator still
receives all original bytes; this is an accounting change justified by its read
shape, not a substituted schema or a decision that a URI is valid. Keep `$id`,
anchors, regex operands and every other text-sensitive constraint unchanged.
Do not extend the optimization to unknown fields or all strings by analogy.

The existing canonical resource/root snapshots must continue to fail on any meta
graph change. Add an explicit audit table for the supported reference-keyword
constraints and fixed non-asserting format policy. Controls include malformed and
unpaired reference strings, long spellings, wrong kinds, unsupported keywords by
edition, and a negative added text-sensitive constraint in the audit guard.
Format is annotation in this fixed admission; actual reference meaning remains
with original resolution.

## Construction-local static query sharing

Retain the existing per-original-node/keyword prepared query map and its exact
effect origins. Add a temporary construction-local lookup that can share the Arc
result for repeated identical reference spellings only where the original query
inputs are equivalent. No cache survives across environments or changes the
registry's authority.

The current `SchemaSpace::query(holder, reference)` reads the exact reference,
the holder's resource identity and its original profile-presence guard; then it
uses that resource's base/anonymous state, eligibility, the immutable namespace
and selected target's fragment inventory. The draft key is therefore original
resource ID + the holder's profile-presence classification + exact spelling,
within that one immutable SchemaSpace. Independently audit every holder-sensitive
read before accepting this key. Including only base URI or normalized text is
insufficient. If the guard cannot be represented completely, retain per-node
resolution for that case.

A temporary key can borrow source text from the retained environment for this
construction call. It does not own another copy of every long spelling. On a hit,
clone the shared query result into the original node's map. A miss performs the
existing resolver under its full reservation before inserting the result. Typed
missing, malformed, ambiguous and unsupported outcomes may be shared only as
static query results; their eventual semantic effect identities remain original.
Dynamic/recursive target selection remains per evaluation's effective scope.

Charge actual hash and equality scans, cache entry/backing growth, Arc creation,
resolver temporaries and per-node map entries before performing them. Prefer an
explicitly sized insertion-only table if a new standard-map allocation schedule
would require an unsupported multiplier. Repeated spellings still require reading
their bytes for identity; this proposal does not claim constant-time deduplication.
Cancellation or any construction exhaustion aborts the same sticky attempt, and
no query result from an unfinished construction becomes a usable prepared owner.

## Gates

Review the read-shape and query-context proofs separately before implementation.
Replay unchanged OE-05 inputs and all 1,546 required values through the original
engine. Include equal spelling under distinct bases/resources/profiles, exact
versus normalized contained IDs, local anonymous resources, invalid identifier
scopes, ambiguity, dynamic anchors and recursive references. Preserve original
failure origins even when their static result Arc is shared. Compare requested
and peak allocations separately from timing, enumerate cancellation boundaries,
and exercise exact/one-below work/allocation and concurrent retained calls.

Native current/minimum Rust and actual Wasm/browser behavior remain required.
Syntax facts shared across distinct preparation phases and full pattern VM-once
preparation are separate open work; this proposal does not quietly claim either.
