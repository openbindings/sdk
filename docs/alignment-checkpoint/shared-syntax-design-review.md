# Independent shared fixed-syntax-fact design review

**Design gate: PASS for the bounded implementation described here. No implementation qualification is claimed.** No production changes or new public API are needed. Review uses the exact current source identities in `source-hashes.json`, pinned spec c1419c9, and the previously qualified localized fixed-checker graph/read-shape contract. The separate prepared-VM certificate remains NOT APPROVED/open.

## Scope and reason

A successful localized fixed-standard check is a pure fact about one original value and its standard edition. The immutable SchemaSpace already fixes both original node identity and profile. Reusing that fact within one preparation can avoid repeat work without changing admission. This follows the original-node/resource boundary in pinned §5.5 (lines 443–459) and §7, and does not replace complete schema admission. The OBI-10 requirement at line 804 still includes contained subschemas; external actual-meta admission retains its existing complete original-source obligations.

Current `preparation.rs::check_from` checks selected activated standard supplied/foreign nodes; `meta_syntax` separately walks every declared position, including unused definitions/content/orphan conditionals. `meta.rs::validate` constructs Plans for selected original metas, checks all supplied resources those Plans use, and separately tracks Checking/Valid/Invalid custom obligations. **Only the call to `dialect_validity_bounded` can be shared.** Those distinct traversal and obligation operations cannot be replaced by a bit.

## Concrete private organization

Prefer the parent's revised construction object owning one `SchemaEnvironment` clone, the bit vector, and the borrowed shared `Control`. `Plan::new` creates it; `Plan::at` takes it and obtains/clones the environment from it. Nested meta Plans and `meta_syntax` use the same object. This prevents mismatching a table and environment and avoids move/borrow pressure without retaining a self-reference. A completed Plan must contain no construction/session/control/fact-table field. Do not start a nested Attempt for each meta Plan.

A private `check_standard(original_node)` operation should derive profile and value only from its owned environment. No caller-provided edition/profile/URI is accepted. Unknown/custom profiles do not read or publish bits; callers retain their existing skip/refusal behavior. If a total-return helper is preferred, encode inapplicability distinctly rather than returning true for custom/unknown nodes. Original usize node IDs are private and must originate in this same environment; the owning object is the association, not a pointer-address key.

On a miss, invoke the actual guarded bounded checker unchanged. Publish a positive bit only after it returns true and the shared attempt's post-check succeeds. No fact for false, an error, cancellation or exhaustion. On a hit, check the attempt/cancellation and charge fixed lookup work. Final preparation post-checks remain authoritative. A bit expresses **local standard syntax true**, even when the schema is literal false; it expresses neither validity of an instance nor completion of a subtree/custom obligation.

Keep the current caller predicates around this helper. For example `check_from` currently excludes ineligible/inventory-refused nodes before syntax checking and consumes contained-2020 conformance separately; `meta_syntax` checks resource eligibility, profile and depth then discovers all original children. The optimization must not broaden either caller's admission applicability, populate bits from eligibility alone, or mask any later resource/dependency error. Existing contained-proof short-circuiting need not manufacture a bit for a checker it did not run.

## Exact storage and resource contract

For N admitted nodes and W=`usize::BITS`, use `words = N/W + (N%W != 0)` with checked arithmetic and `bytes = words * size_of::<usize>()`. This avoids the overflow of `(N+W-1)/W`. Reserve the entire requested backing and initialization work before allocating/zeroing; check arithmetic and propagate sticky refusal through the same Attempt. No per-node map or guessed HashMap multiplier is needed. Zero words should allocate nothing. Charge fixed lookup/update operations; do not rescan all nodes to compute already-stored `node_count`.

Map only `node_number(original_id)` into this table. `SchemaSpace::nodes` enumerates local then packaged nodes, and `node_number` adds the local length to packaged IDs after removing the top-bit marker. It is dense for valid IDs; raw packaged IDs are not vector indices. Node values, profiles, aliases and selected custom carriers cannot change during construction. Separate environments, source replacement or separate top-level calls get fresh tables even when spelling and node numbering happen to match.

This dense table is a new temporary allocation, so exact/below-limit tests must measure the complete new operation rather than preserve an incidental old budget threshold. It does not authorize increasing any default or weakening semantic/workload gates. Existing validator cold-start reservations and local source extent remain on misses; a hit avoids those operations and their charges because it actually avoids the work. Initialization, table ownership and hit checks remain charged. A local-positive table need not be global or retained in warm immutable environments.

## Required independent controls

`controls.json` specifies 26 groups; `shared-meta-input.json` is an exact two-meta/shared-helper input. The most important distinctions are: a locally valid helper whose unused child is invalid; a valid-syntax false/rejecting helper; custom/standard same-text operands; different profiles; packaged dense indices; a cached parent's nested custom dependency; source replacement; and custom Checking cycles. These are required before implementation PASS, not observations of an implementation that does not exist yet.

Instrument the fixed checker in a private test bridge to prove the shared helper is checked once while actual meta obligations and child/dependency traversal remain. Do not infer execution counts from timing. Check all publication cancellation boundaries, exact/one-below and ignored nested exhaustion, fresh retry, and allocation release. Retain current fixed graph snapshots/read-shape controls, all 1,546 forced-original semantic cases, custom-meta budget controls, original frozen reference workloads, minimum/current Rust and actual host lanes.

## Findings and remaining boundaries

No blocking conceptual finding in this bounded design. The owned construction-object refinement is approved. Passing implementation review still requires the concrete storage/publication and traversal tests above. This change does not certify full VM-once preparation, the outgoing general compiler, final default-engine replacement, diagnostics or overall SDK/performance completion.
