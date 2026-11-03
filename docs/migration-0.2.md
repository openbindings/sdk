# Current 0.2 foundation checkpoint

This unpublished migration treats existing APIs and module boundaries as replaceable.
The selected target specification is
`c1419c9fc3ae5a00c2b83da710c11bf67dab7934`; target interfaces are
`5815fb106bddd2c85ff4e93e0077570b3f6b9e74`. The interface companions have
not yet adopted that target revision.

The first implementation slice updates document admission:

- Source and binding fields/accessors use `data`, preserving absence, explicit null
  and exact JSON. No `content` alias is provided.
- The derived structural schema and all 505 core cases come from the target spec.
  Validation reports evidence for OBI-01 through OBI-14.
- A document pointer can identify any contained schema, including schemas inside
  identified resources. Primitive values at malformed schema slots remain non-schemas.
- Contained-resource references use the exact compared identifiers from OBI-13.
  Initial static resolution applies to both reference keywords; core does not fetch.
  Unknown-keyword data is not a schema. An unusable compared base leaves that
  resource's references outside OBI-12's second clause.
- Duplicate identifiers cannot select a resource; used duplicate document anchors
  cannot select a schema. Both OBI-12 and OBI-13 can be violated. Identical copies
  remain distinct occurrences; there is no first/last-writer rule.
- OBI-14 rejects every dynamic-anchor member in the document resource, including
  unused declarations. Identified schema resources retain JSON Schema semantics.

The reference checks share the compact identifier representation used for duplicate
checking. They retain source locations without expanding every nested relative URI.
The schema inventory owns this interpretation; document assessment collects its
rule evidence. This is the first portion of the future core schema responsibility.

Native qualification covers the full current 505-case corpus, 30 adjudicated
historical admission cases, core/internal-schema tests and document-rule controls.
The regression suite includes a 4,000-level relative-ID/reference chain.
These results do not establish complete evaluator capability or host parity.

The ownership work makes edits return immutable snapshots, replaces the generic
proof with concrete ConformantDocument, exposes a conformance module, and adds exact
owning JsonObject access. Rust Edit values now own paths and values. Contextual Schema
views replace borrowed ValueContract handles, cover named schemas and operation slots,
and retain their document after caller handles are dropped. TypeScript exposes the
same retained Schema capability. Independent ownership reviews pass for these slices.

The reviewed 1,546 value cases and 20 moved admission negatives are now checked into
the SDK with exact input-accounting controls. Unicode property/terminal-lookahead
support and exact contained-resource selection pass separate independent reviews.
The current strict evaluator gate still has 10 external dialect/vocabulary gaps;
an additional independent identity corpus exposes 8 semantic-undefined capability
gaps. The full workspace and final performance gates remain open.

The next slices replace parallel typed parts/views with owned domain views and expose
the original schema environment through the redesigned evaluator interface. Retired
role companions still await replacement by current invocation, context, kind
description and source pulling. See [the development loop](alignment-loop.md) for
open gates and the unattended work policy.

No release, compatibility promise or completion of the migration is implied by this
checkpoint. The canonical plan, frozen inputs, design reviews and measurement receipts
are maintained in the adjacent sdk-migration repository.

The original-schema environment is implemented and independently reviewed. Resource
inputs are `ExternalSchema`/`ExternalSchemas`; original diagnostic coordinates are
`SchemaOrigin { source_uri, pointer }` in Rust and `{ sourceUri, pointer }` in
TypeScript. These coordinates identify original source text, independently of the
resolved `$id` base and opaque environment-scoped resource identity.

`Schema::environment` takes explicit external inputs, `PreparationLimits`, and a
cooperative work control. Original nodes retain their owners, and queries do not fetch.
Unknown external dialect roots remain inspectable, while operations requiring an
unsupported inventory return a capability failure. Construction is bounded before
URI expansion, including external registration and cached-environment reuse. The
remaining generic evaluator migration will replace the old projection-based seam;
this checkpoint does not claim complete value-evaluation qualification.
