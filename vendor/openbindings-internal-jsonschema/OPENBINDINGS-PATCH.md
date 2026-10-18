# Internal dependency fork

Based on `jsonschema-0.58.6`; upstream license and notices are retained.
The distinct package identity selects the qualified work/depth/cycle and
ECMAScript-regex guards. The regex correctness fixes (empty quantifier semantics
and backtrack budget restoration) remain in the sibling regress fork; see
DEPENDENCY-MAINTENANCE.md.
One correction is the fork's own: an `unevaluatedProperties` walk keeps the unsettled
`patternProperties` subschemas it compiles, which upstream drops although a later
compilation round can still reach them through a non-owning edge.
All diagnostic admission, copy, payload, metadata and schema-detail changes have
been removed. Error construction is the upstream implementation.

The SDK uses documented keyword and value-representation APIs for exact decimal
semantics. jsonschema-value, referencing and serde_json are unmodified registry
packages; this fork no longer requests arbitrary precision for SDK builds.

The repository maintenance contract is DEPENDENCY-MAINTENANCE.md, including
per-file upstream line counts and the reason each source change remains.
UPSTREAM-VCS.json and UPSTREAM-Cargo.toml retain the original identity/manifest;
these names avoid Cargo-reserved packaging metadata. The complete reproducible
patch and archive checksum are in docs/dependency-patches at the repository root.
This package is an implementation detail, not a supported SDK extension API.
