# Sharing completed fixed syntax facts during original preparation

Independent design review passed; implementation and host/resource qualification
are in progress. See `../../receipts/2026-11-02-preparation/shared-fixed-syntax-facts-review/design/review.md`. This is narrower than schema admission: it shares only successful local fixed-standard syntax checks within one
completed immutable SchemaSpace. It does not share custom-meta verdicts, reference
eligibility, resource closure, dynamic scope, or arbitrary validator outcomes.

## Repeated work in the current source

`preparation::check_from` checks each activated standard foreign/supplied schema
position. `meta::validate` constructs additional Plans for selected meta targets,
then calls `meta_syntax` across every supplied resource those Plans use, including
unused definitions. A position reached by both traversals, or by multiple meta
Plans, can run the same localized fixed checker repeatedly. The complete traversal
is still required: evaluation reachability alone does not establish admission of
unused schema positions or custom dependencies.

The outgoing projection path also checks syntax, but it is scheduled for removal.
Do not introduce a new proof/cache API into that obsolete path merely to preserve
its architecture. Existing conformant-document admission remains authoritative for
contained 2020-12 schemas where the current engine already consumes that fact.

## Proposed ownership and identity

Create one private construction object in `Plan::new`. It owns a clone of that call's
immutable environment and borrows its shared attempt and shares a fixed-syntax fact table across root `check_from`,
meta Plan construction and complete meta-resource syntax traversal. Completed Plans
retain neither the session nor its control. Each new top-level preparation gets a
fresh session; no mutable global or cross-source certificate is introduced.

A table bit means only: this exact original node, under its immutable standard
profile/edition in this environment, passed the complete local fixed checker.
Node identity includes its actual source record and resource interpretation. No
URI or text normalization participates. Custom/unknown profiles never read or
populate these bits. Set a bit only after validation returns true and the attempt's
post-check succeeds; false/cancelled/exhausted/incomplete checks publish no fact.

Use a temporary `Vec<Cell<usize>>` bit vector (single-threaded, never retained by a Plan) over `SchemaSpace::node_count()`, indexed by the
existing compact `node_number(original_id)`. Raw packaged IDs use the top bit and
must never be used as vector indices. This keeps whole-environment bookkeeping
small without an arbitrary cache cap: approximately one bit per admitted node,
including the immutable packaged layer. Checked allocation/work covers full backing
and initialization before they occur. Overflow or reservation failure marks the
same sticky Attempt. Lookup still checks cancellation and charges its fixed work.

The session should own the association between environment and table; callers
cannot pass a different space to a lookup. Avoid pointer addresses as identity or
a public proof type. Refactor the private preparation signatures as needed; none
of the existing internal APIs has compatibility protection.

## What must still execute on a hit

Keep each traversal's resource eligibility and custom-meta selection checks,
profile handling, depth/size rules, original child discovery and dependency
collection. A completed local fixed-syntax fact is not a subtree certificate.
In particular, `meta_syntax` must still visit unused definitions, content schemas
and orphan conditionals. Passing a standard root cannot admit its descendants.
Actual custom-meta evaluation and its recursive obligation state remain separate.

No failed source lookup or invalid profile can be hidden by a syntax fact. A
same-text node in another original position or changed environment has a separate
identity. Draft 7 reference-object traversal behavior and the guarded local read
extent remain unchanged.

## Gates

Independently fix cases with one helper reached by two meta Plans, standard roots
whose unused child fails syntax, nested custom metas, orphan conditional/content
schemas, shared source text under distinct profiles, packaged/local node indices,
source replacement and different selected roots. Show the same positive fixed
check executes once while the complete required child/dependency traversal remains.

Measure requested backing and temporary owner cleanup. Exercise zero/exact/one-below
work/allocation, cancellation before/after validation and before publication, ignored
nested exhaustion, and fresh retry. The facts table cannot outlive preparation or
turn a failed attempt into Ready. Retain the 1,546 forced-original cases, existing
custom-meta certificates/budget lanes and frozen reference workloads. Run current/
minimum Rust and actual Wasm/Node/browser gates. This does not qualify pattern VM
programs, original diagnostics, the outgoing compiler or the final default switch.
