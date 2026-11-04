# Reference bundle and acquisition-boundary repairs

Hosted qualification at `2584b8b` exposed a stale lockfile in the standalone
no-I/O consumer and four README-relative links in the generated Rust reference.
These repairs retain the strict locked build, complete reference-link checker,
and HTTP/FIFO positive controls. No production evaluator behavior or compatibility
requirement changes.

The no-I/O lock adds only the already-enabled `jsonschema -> idna` and evaluator
`-> regex-automata` edges. Every package version, registry source and checksum is
unchanged. The locked build and strict Clippy check pass.

The first rebuilt no-I/O run reached the SDK and exposed an obsolete assertion
that an opaque external custom profile must produce `UnsupportedDialect`. The
already-reviewed current policy refuses unproved external namespace selection
with `UnsupportedCapability(custom-profile-namespace)`. The exact original input
is retained. An initial proposed `ResourceUnavailable` assertion also failed at
that earlier namespace guard; serialized diagnostic inspection resolved the
mistake before accepting the control. A separate supplied known custom profile
now drives meta-schema admission to the trap URI, requiring the exact structured
`ResourceUnavailable` cause there. This distinguishes namespace uncertainty from
established missing bytes instead of accepting any failure.

Both HTTP and FIFO controls pass: the raw library fetches its false resource,
while SDK evaluation makes zero retrievals and passes five outcome checks in
each mode. These include ordinary missing references, opaque profiles, known
meta-schema dependencies, uninterpreted kind/data, and explicitly supplied false
schemas. The original failing receipt is preserved alongside the passing one.
This is an active regression witness, not a universal proof of absence of I/O.

The generated reference binds three known source-link spellings (four link
occurrences) to the exact candidate commit on GitHub and records every resolution.
Missing source files and unrelated broken links still fail. The reference home
also lists the current schema-comparison, context, kind-description, invocation
and source-pull companions. Four focused tests pass, including negative controls.
An 804-file preview bundle passes the complete local link check. The first-use
example's remaining per-value contract wording now says schema; its build passes.

Independent review and the clean committed reference candidate are pending.
The complete SDK alignment, final matched performance, engine replacement and
all-host qualification remain open. No package was published or deployed.
