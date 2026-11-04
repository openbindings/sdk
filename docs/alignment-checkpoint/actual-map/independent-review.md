# Independent current alignment audit

Reviewed production checkpoint: `df06588bb8c07251b2f15e5121f2456634c99dfe`.
Authority: the pinned core `c1419c9` and interfaces `5815fb1` under
`inputs/current-c1419c9-5815fb1`, with the already adjudicated SDK designs and
bounded implementation reviews. This is a current source/API/module and evidence
audit. It is **not** a whole-SDK, performance, arbitrary-provider, or exhaustive
semantic PASS. No SDK source was edited for this review.

## Result and actionable findings

The current API and package boundaries represent the pinned concepts without a
new substantive model or role-authority conflict found in this bounded pass.
All 32 declared fields and all 19 mapped relations still have actual owned APIs;
the old sketches' particular names or detached representations are not required.
The default value engine is now one original-node Plan. The map must describe
that completed implementation change rather than its historical two-engine state.

1. **R-ALIGN-DOC: current public descriptions are stale.** At the reviewed
   checkpoint, `CAPABILITIES.md`, the evaluator README's ownership/backend-gate
   paragraphs, and `docs/architecture.md`'s default implementation section still
   describe an ordinary compiled lane plus semantic fallback, or replacement as
   future work. `DefaultValidator` actually retains `Arc<semantic::Plan>` and
   every value call uses that Plan. Original immutable environments, prepared
   queries/operands/programs and per-call mutable scratch are the current model.
   Fixed normative-meta/format validators and immutable regex compilation remain;
   removing the ordinary schema compiler does not mean all dependency compilation
   disappeared. Parent is correcting these public descriptions.
2. **R-ALIGN-MAP: historical map identities need a current overlay.** All 888
   recorded source/test file-anchor references resolve, with no missing file or
   missing anchor. There are 98 stale recorded hashes across 12 files. Hash drift
   is not by itself a defect, but those records cannot attest the new runtime.
   The package map still lists deleted evaluator implementation modules and the
   design's module notes still call the compiler transitional. Preserve their
   historical source pins; apply the attached current package/field/hash overlay
   and the current original-engine qualification receipts before claiming a
   current complete binding. Existing witness names are not substitutes for their
   actual assertions or runtime results.
3. **R-CAP-PATTERN: the old large expanded-pattern positives are not closed.**
   The `diagnostic-memory-probe` single/within-total/repeated-pattern jobs using
   5,000 `\\S` atoms previously require successful preparation and S on `{}`.
   Current default preparation exhausts SDK work; the repeated 380-occurrence
   case still exhausts SDK reservations with funded work. No reviewed structural
   policy disposition found in this audit explicitly withdraws these positives.
   Funded isolation lanes are useful evidence but do not turn default refusal
   into completion. See `pattern-workload-adjudication.md`. Counted-pattern and
   negative total-extent cause changes have a different disposition.

No new wrong S/F/U result was established in this pass. These findings leave
documentation/evidence currency, the named capacity obligation, and the parent's
matched performance gate open. The parent's pilot reports allocation regressions;
neither that pilot nor this map audit grants residual acceptance.

## Coverage and method

The audit considered the existing 54 concepts (11 document, 15 schema/conformance,
11 context/invocation, 17 companion/host), 83 obligations (54 semantic, 14 document,
15 interface/discovery), 108 individual witnesses (52 document/schema, 56 companion),
32 fields, 19 relations, and 13 Rust package manifests. File-anchor integrity was
checked mechanically, followed by source review of current owners, exported APIs,
package dependencies and the parts affected by default-engine replacement.
Earlier independent implementation reviews supply the detailed control evidence;
this pass did not rerun every old witness or infer correctness from anchor existence.

`authority-integrity.json` also checks 137 quoted authority-file/text bindings
against the actual pinned core/interface input tree. All files and quoted anchors
exist. This guards stale pin references; it does not elevate a matching quotation
into a proof of implementation behavior.

`field-overlay.json` independently resolves each field's authority JSON Pointer
against the pinned core schema. The declared property sets of the root and the
five entry definitions contain exactly the mapped 32 fields: no missing or extra
field, and both Rust and TypeScript API anchors exist. `package-overlay.json`
comes from actual current Cargo manifests and source roots, separating normal,
development and target-specific edges. `binding-integrity.json` and
`hash-drift-overlay.json` retain each historical binding and its current identity.

## Concept and relationship conclusions

- **Document and ownership.** Raw `Document` remains immutable exact JSON and can
  represent drafts. Only completed document conformance creates the proof from
  which canonical Operation/Source/Binding/Dependency/Example owners are acquired.
  Entries retain that immutable snapshot. Editing makes a new snapshot; a proof
  or prepared owner cannot silently migrate to it. There is no replacement raw,
  detached authoring-parts or duplicate conformant domain hierarchy.
- **Fields and names.** Optional namespaces and fields preserve absence, empty,
  false and permitted null separately. Source/binding opaque data and examples
  remain exact owned JSON. Operation primary-key selection is distinct from
  explicit alias resolution. Relations use keys in the same proof. Preference is
  a proved, bounded integer carried as `i64`; its exponent spelling does not
  become a separate domain concept. Kinds, dependencies and realizations do not
  infer trust, support, ranking or execution permission.
- **Schemas and evaluation.** Schema identity is placement/original-environment
  based; examples and opaque data do not become schemas. ExternalSchemas are
  explicit immutable inputs with contained/protected precedence, original
  dialect/base/dynamic scope, and no implicit retrieval. Core document checks and
  value evaluation are separate. Preparation failure, F, U and NoVerdict remain
  distinct. Required custom-meta admission is not ordinary missing evaluation
  content. Plan-only preparation retains actual originals and immutable artifacts;
  per-call proof/search state is not shared mutable prepared state. Explain
  preserves an established verdict and may produce incomplete original-location
  diagnostics; its diagnostic spelling is not a legacy compatibility contract.
- **Current roles.** Binding/operation clients retain explicit selected keys and
  original proof, with per-value satisfaction required before delivery. Portable
  invocation errors retain code and optional data, not an invented message field.
  Preflight has the published same-input ERR_REFUSED prediction exception; it is
  otherwise advisory and is not automatic invocation. Context acquisition and
  retries remain caller decisions. Borrowed provider futures, cancellation and
  cleanup remain the separately reviewed finite/streaming lifecycles.
- **Kind and pull boundaries.** Per-operation support and check disclosure are
  claims, not universal kind certification or permission. Source pulling checks
  mechanical relationships and prospective conformance while preserving the
  original document; proposed edits remain unapplied until the caller acts.
  Provider fidelity and opaque dependency completeness cannot be proved by a
  generic client. Existing-entry drift uses the kind's would-write comparison;
  added-binding reuse may use the independent schema-identity profile. Identity
  is neither general equivalence nor directional compatibility.
- **Hosts and packages.** There are 11 logical products including TypeScript and
  13 Rust packages because three version-locked private packages share exact JSON,
  schema interpretation and cancellation. Core has no optional value-evaluator
  or role production dependency. Invocation/kind/pull do not force the default
  evaluator. Source-pull comparison is a provider/example development edge, not
  a generic production rule. Wasm depends on the default evaluator; TypeScript
  exposes the checked domain/schema facade and optional discovery entry, not a
  second semantic engine or fabricated public Rust-role bridge. The old
  inspection/synthesis product packages remain absent.

## Resource and evidence limits

The pinned security section requires attention to size, matching, identifier
isolation and recursive traversal. It does not require the retired dependency-wide
source-word/cumulative allocator certificate. The current public SDK counters,
structural compiler admission, representation footprint, per-call scratch and
measured process behavior have distinct scopes. The adopted greenfield correction
can change unpublished policies, but a faster resource refusal is not successful
completion of a retained positive consumer job.

Historical performance/stack/implementation receipts that explicitly name old
revisions are valid history and need not be rewritten. Final portable/native/
minimum-Rust/Wasm/host, matched performance and default-capacity qualification must
name the final source and preserve failures. This audit establishes the bounded
concept/API/source conclusions above, not those still-running gates.
