# Claude review reconciliation

The independent Opus 5.5 Extra [review](claude-review.md) examined implementation
commit `6137585efe220f49a96fadd5af8e0504aacc38f1` on 2026-10-13. The reviewer received
current code, fresh acceptance cases and parent-run evidence; previous binding
drafts and the Astra conclusions were excluded. Its report is retained verbatim.
This file records the coordinator's dispositions, rather than attributing agreement
with every sentence of that report.

## F1: schema dialect — repaired and independently verified

The new public-consumer regression reproduced the incorrect operation projection
under a draft-07 document default. The repaired adapter uses the client's lexical
`schema_description().profile` before deriving a 3.1 scalar type. The OAS 3.1 base
and dated base dialects and JSON Schema 2020-12 are admitted. Other parameter
dialects are implementation-unsupported, with a stable coverage reason, no generated
operation and no dispatch for either numeric or string input.

Nine regression rows cover document defaults, local overrides in both directions,
opaque dialects and all three accepted identifiers. Accepted projections still
report their unresolved source-schema constraints as loss. No general validation,
dialect support expansion, role/core change, client change or binding rule was added.
The [red](claude-repair/dialect-red.log) and [green](claude-repair/dialect-green.log)
runs are coordinator evidence. An initial test compilation typo was corrected
before the recorded red runtime test; it did not exercise the defect.

The basis is [OAS 3.1.2 schema dialect selection](https://spec.openapis.org/oas/v3.1.2.html#specifying-schema-dialects)
and [draft-07 reference-object semantics](https://json-schema.org/draft-07/draft-handrews-json-schema-01#rfc.section.8.3).
The coordinator does not adopt the report's assumption that a server necessarily
returns an error for the misinterpreted input. Server behavior is not established;
refusing the unsupported profile before dispatch is the supported guarantee.

## F2: single-entry cache — accepted limitation

The current provider caches one source. Alternation reparses/re-analyzes each time;
analysis holds the mutex; failed preparation can evict the cached entry. These
facts are now explicit in the package README. Separate providers per source are
available through the existing API. The current warm measurements apply only to
repeated use of one source. No alternating-source latency or contention measurement
was made, and there is no claim that the single-source numbers represent it.

A bounded multi-source cache and analysis outside the mutex remain potential
performance work, requiring an alternating-source workload and memory budget.
They are not interface or binding-spec requirements and do not block this slice.

## F3: CPU evidence — qualify the conclusion

The registered gates have substantial headroom. They are broad overhead budgets,
not proof of competitive performance, zero regression or HTTP latency parity. The
literal statement that they cannot fail is too strong: exceeding their finite
ceilings would fail. We retain the original gates and observations, and report
measured adapter/direct ratios and paired overheads alongside their outcomes.
No gate was tightened retrospectively or relaxed to make a result pass.

## Other observations

- Clarified that the status-only projection-loss statement covers explicit non-2xx
  responses. A `default` response can cover success and must meet the supported
  success profile; the conservative implementation remains unchanged.
- Node/Wasm tests qualify exact-text crossing, asynchronous ownership and cleanup.
  Error paths do not independently assert the native zero-effect guarantees;
  native effect-boundary regressions are separate evidence. No browser network claim.
- The 3.0 `webhooks` conservatism and source-error category wording are retained as
  minor follow-ups. They do not silently dispatch or invent represented operations.

Claude's [focused addendum](claude-repair-review.md) confirms F1 is closed and finds
no new material issue. It also withdraws the unsupported server-response assumption
and the absolute wording about CPU gates. This is static independent verification;
Claude did not run tests or measurements.

Three additional nonblocking dialect limits remain explicit: malformed document
`jsonSchemaDialect` can be ignored by the pinned client in favor of its edition
default; equivalent URI spellings outside the exact accepted set are refused; and
layout-typed fragments use their own client interpretation context rather than
inheriting an entry document's dialect default. The last path has not received a
new dialect-specific fragment regression here. These are retained follow-ups, not
expanded support claims or binding rules.

Refreshed qualification is recorded under `claude-repair/`. This review does not
authorize merge or publication.
