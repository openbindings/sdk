# Internal dependency fork

Based on `jsonschema-0.58.6`; upstream license and notices are retained.
The distinct package identity selects the qualified evaluation work/depth/cycle,
compilation work and ECMAScript-regex guards, weighs compilation work by what each
build does, caches compiled schemas by location alone in builds that reach no dynamic
anchor, and releases a build that stops part way without recursion. Such a build
fails rather than compile a reference it resolves through a dynamic or recursive
anchor, from the reference keywords or an `unevaluatedProperties` or
`unevaluatedItems` walk; the `oneOf`/`anyOf` discriminator follows only `$ref`s the
`$ref` keyword also compiles, under the standard vocabularies. The regex
correctness fixes (empty quantifier semantics and backtrack budget restoration)
remain in the sibling regress fork; see DEPENDENCY-MAINTENANCE.md.
One correction is the fork's own: an `unevaluatedProperties` walk builds only the
`patternProperties` patterns it needs. Upstream also compiles and drops the subschemas,
although a later compilation round can still reach their unsettled `$ref` targets through
a non-owning edge.
All diagnostic admission, copy, payload, metadata and schema-detail changes have
been removed. Error construction is the upstream implementation.

The SDK uses documented keyword and value-representation APIs for exact decimal
semantics. jsonschema-value and serde_json are unmodified registry packages;
referencing is the sibling referencing fork, which makes resource registration
linear. This fork no longer requests arbitrary precision for SDK builds.

The repository maintenance contract is DEPENDENCY-MAINTENANCE.md, including
per-file upstream line counts and the reason each source change remains.
UPSTREAM-VCS.json, UPSTREAM-Cargo.toml and UPSTREAM-Cargo.lock retain the original
identity, manifest and lock; these names avoid Cargo-reserved packaging metadata and
a lock that standalone Cargo commands would rewrite. The complete reproducible
patch and archive checksum are in docs/dependency-patches at the repository root.
This package is an implementation detail, not a supported SDK extension API.
