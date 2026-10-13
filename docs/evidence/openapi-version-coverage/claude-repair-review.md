# F1 repair addendum

Reviewer: Claude Opus 5.5 (Extra), 2026-10-13. Input: `claude-repair-input.txt`, which contains the patch and the coordinator's red/green report.

I also re-read the pinned client's dialect code (`client/src/schema.rs`), which the original allowlist permits. I ran no tests. Pass and fail results are the coordinator's.

## Verdict

**F1 is closed.** This repair does not block closeout of the bounded slice.

## Why the repair closes F1

**The guard runs at the right point.** For the 3.1 kind, `profile()` in `analysis.rs` (≈276–286) now reads the client's `schema_description().profile` before it trusts `type`. That profile is the client's lexical choice:
- a schema-level `$schema` comes first;
- otherwise, `jsonSchemaDialect` set at a real OpenAPI root;
- otherwise, the edition default, which is the OAS 3.1 base dialect.

So the original trigger now produces `parameter-schema-dialect-unimplemented`, emits no operation and dispatches nothing. That trigger is a document whose default dialect is draft-07, or a local draft-07 `$schema`, combined with `$ref` and a sibling `type`.

**The accepted set is safe.** The three accepted URIs are the OAS 3.1 base dialect, its dated identifier and 2020-12. That is exactly the client's own `DialectCapability::Supported` set for 3.1 (`schema.rs:50–57`). If a spelling in that list were wrong, the only consequence would be a conservative refusal, never a false representation.

**The accepted branches keep only the existing lossy scalar projection.** In 2020-12 and the OAS base dialect, keywords at the same level apply together. An explicit scalar `type` is therefore a necessary condition of the effective schema, so `{"type": t}` describes a superset of the values the schema allows.

Any other member (`$ref`, `$schema`, constraints) sets `schema_loss`, and the test checks `!fully_represented()`. Dispatching `"abc"` in the represented rows confirms that no value validation is claimed. Bodies and responses remain unprojected, so nothing else depended on the dialect.

**The regression test covers the cases that matter.** The nine rows cover document default against local override in both directions, references with and without siblings, and opaque, legacy, base, dated and 2020-12 dialects. Each refusal row also checks the reason code, refusal before effects and zero dispatches. The coordinator reports that the test failed on the old runtime at its first row and passes after the patch.

**No new material issue.** The change is a narrowing confined to the 3.1 kind. The 3.0 path is unchanged, and core, role and client code are untouched. The README accurately describes both the guard and the narrowed `default` wording.

## Corrections to my original report

- **F1:** I wrote "No unsafe effect follows: the server would answer with a status error." Withdraw that sentence. Server behaviour cannot be assumed. The defect was a misderived caller contract, and nothing more should be claimed about its effects.
- **F3:** "Cannot fail" was too strong. The accurate statement is that the registered ceilings admit roughly 6–11× the direct cost at the measured sizes. Recording F3 as permissive evidence, alongside the measured ratios, is right.

## Residual follow-ups (not blocking)

Record these with the other dialect follow-ups.

1. **A malformed `jsonSchemaDialect` is silently ignored.** If the value is not a string or is not a valid URI, the client falls back to the edition default (`schema.rs:526–529`, `let Ok(declared) = profile_from(..)`). The adapter then accepts the parameter as base-dialect. Upstream requires a URI, so such a document is invalid; the issue is lenient admission, not the F1 defect.
2. **Exact-string matching refuses equivalent spellings.** Variants such as `…/2020-12/schema#` are refused. That is a coverage loss only.
3. **Layout-typed fragment resources take the client's edition default.** They do not inherit the entry document's `jsonSchemaDialect`. This follows the client's reading of "Schema Objects contained within this OAS document" and is untested here.

## Closeout recommendation

Close the bounded slice once the coordinator's refreshed affected, package and Node/Wasm qualification is green. Record the following as limitations or follow-ups, not slice blockers:
- F2 (single-source warm cache);
- F3 (permissive regression budget);
- the Node error-path evidence limit;
- the three dialect residuals above.
