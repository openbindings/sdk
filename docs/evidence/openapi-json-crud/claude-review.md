# Ordinary JSON CRUD expansion: independent review (Claude)

Reviewer: Claude Opus 5.5 (Extra), 2026-10-13. Branch `codex/sdk-openapi-json-crud` against baseline `d975cc2`.

**Read:**
- `docs/evidence/openapi-json-crud/plan.md`;
- the diffs of `analysis.rs`, `invocation.rs` and `tests/manual.rs`;
- the new `tests/crud.rs`, `examples/openapi-json-path-consumer/src/crud.rs` and its test, and `crud-fixtures/exchanges.json`;
- the pinned client's (`fe09522`) request-body and response-key handling, to confirm the edition rule.

**Not done:** no builds, test runs or timings. I make no claim that tests pass.

**Excluded:** old binding drafts, which are not an authority here.

## Verdict

The runtime design is sound and narrowly implemented. **Accept the code**, subject to one documentation finding (C1) to fix before landing. Two notes should be recorded with it (C2, C3).

## Verified

- **Methods.**
  - `profile()` now admits PUT, PATCH and DELETE alongside GET and POST. HEAD, OPTIONS and TRACE stay `unqualified-method`, and the tests show implementation-unsupported coverage and refusal with zero host entries.
  - PATCH still requires the single request media type `application/json`, because the existing request-media check is unchanged. JSON Patch and Merge Patch are therefore not admitted.
  - No idempotency claim is synthesized.
- **DELETE request bodies differ by edition.** The adapter defers to the client's effective request-body description.
  - At `fe09522`, 3.0 returns no body for methods other than POST, PUT, PATCH and OPTIONS. That follows the 3.0.4 Operation Object rule, so a declared DELETE body is ignored and a caller body is refused.
  - 3.1 describes the body, and a required DELETE body is enforced.
  - The test asserts both directions with zero dispatches on refusal. No common rule is invented.
- **Response selection.** `EmptyResponses` compiles one bit per 2xx code during analysis:
  - the fallback is `2XX` if present, otherwise `default`;
  - its content-less flag seeds every bit;
  - exact numeric 2xx declarations then set or clear their own bit.

  That is exactly exact > range > default, per the Responses Object. Client key validation (three characters, digits or uppercase `nXX`, or `default`) makes the string matching safe.

  The tests cover exact, range and default precedence, a referenced `$ref` response, and an undeclared status falling back to JSON.
- **Empty versus JSON null.**
  - A selected content-less success with complete, empty bytes produces no output.
  - Non-empty bytes there produce `ERR_EXECUTION_FAILED` after effects; they are never discarded.
  - A JSON-declared or undeclared success requires valid JSON, and JSON `null` produces one `"null"` output.
  - Statuses 204 and 205 always require empty bytes, whatever the metadata says.

  All of these cases are in `tests/crud.rs`.
- **Complete response evidence.** The empty path runs only after the existing outcome checks, which require dispatch, no error, a complete body, complete headers and zero omitted header lines. Truncated bodies, unspecified header delivery and cancellation are tested and never become empty success.
- **Resources and lifetime.** The policy is a `Copy` `u128`, compiled once per profile and charged to the retained bytes (`size_of`). It is carried in the invocation future instead of retaining a cloned request or credentials, so no secret or second payload owner lives across execution.
- **Boundaries.**
  - No core, role or published interface changes.
  - Generic invokers are unchanged.
  - The native HTTP test covers PUT (exact large integer), PATCH with a null body and an empty 202, and DELETE with 204 and 205, for both editions.
  - The public consumer runs six literal exchanges through both binding and operation invocation, for manual and synthesized-then-reloaded documents.

## Findings

**C1 (material, documentation). The package README still describes the previous contract.** `crates/openbindings-openapi/README.md` is unmodified and still says:
- "the qualified profile uses GET/POST" (line ~187);
- "Explicit non-204 no-content success declarations are currently unsupported" (line ~191);
- "a content-less … `default` makes the operation implementation-unsupported" (lines ~195–196);
- that only "a verified empty 204 produces no value" (line ~69).

`lib.rs` says the private kinds' meaning "is documented in the package README", so the landed code would contradict the kinds' documented meaning.

**Fix:** update the README with:
- the five methods;
- the 3.0 versus 3.1 DELETE request-body rule;
- exact > range > default selection;
- the content-less-success-requires-empty-bytes policy;
- 204 and 205 always empty;
- the unchanged status-only error mapping.

No code change is needed.

**C2 (record). The existing r1 kinds' correspondence is extended, not revised.** For bindings that were already callable, the only changed outcome is HTTP 205 with empty bytes: it was `ERR_EXECUTION_FAILED` before (JSON was required) and is now a no-output success. Every other change admits targets that were previously refused. That is defensible as an HTTP-aligned correction under the same labels. Because earlier work set a policy of revising the label when frozen meaning changes, record the decision explicitly in the README and decision record: compatible extension, no r2.

**C3 (record as implementation policy, not upstream mandate). A content-less `default` now governs undeclared 2xx statuses.** This follows Responses Object precedence. But a content-less `default` usually documents errors.

Example: `200` JSON plus `default: {description: "error"}`. A server answering an undeclared 201 with a JSON body now gets `ERR_EXECUTION_FAILED` after effects, rather than the JSON value.

OAS describes `content` as potential payloads and does not state that omitting it guarantees an empty body. "Selected content-less success requires empty bytes" is therefore this adapter's policy. The plan chose it deliberately, and the tests cover it. Document it as such, and list it as a portability observation rather than a requirement on any generic role or future binding specification.

## Portability versus policy (for the decision ledger)

| Item | Classification |
| --- | --- |
| exact > range > default selection | Upstream (Responses Object) |
| 3.0 ignoring DELETE request bodies | Upstream (3.0.4 Operation Object) |
| 204/205 carry no content | HTTP (RFC 9110) |
| Empty bytes required for a selected content-less declaration | Adapter policy (C3) |
| JSON required when no declaration matches | Adapter policy (C3) |
| Status-only error mapping | Adapter policy |
| Refusal of HEAD, OPTIONS and TRACE | Adapter policy |
| JSON-only PATCH | Adapter policy |

No generic role requirement follows from any of these.

**Disposition:** accept after C1; record C2 and C3.

## Final reconciliation (2026-10-13)

Read-only check of the updated package README, the CHANGELOG entry, `docs/evidence/openapi-json-crud/portability.md` and the new `tests/crud.rs` sections. No builds, test runs or timings. Targeted package, native and Wasm qualification is still running, and I make no claim about its results.

**The runtime is unchanged since my review.** The diff against `d975cc2` is still `analysis.rs` +44 and `invocation.rs` +22 lines. Current sha256 prefixes are `9a69406b` and `b31a183d`. The findings above are kept as history; corrections follow.

- **C1: resolved.** The README now states:
  - the five methods, and HEAD, OPTIONS and TRACE as outside the profile;
  - application/json-only PATCH;
  - that 3.0 ignores GET and DELETE request bodies while 3.1 keeps them;
  - exact > `2XX` > `default` selection;
  - one value for JSON null, and no value only on verified empty bytes for a selected content-less declaration;
  - that 204 and 205 are always empty;
  - the JSON fallback for unmatched statuses, explicitly not full conformance validation;
  - the policy caveat;
  - that the r1 labels are retained.

  `lib.rs`'s pointer to the README as the kinds' meaning is now accurate.
- **C2: corrected.** I understated the 205 change for operations that were already callable. It changes outcomes in **both** directions:
  - an empty 205 moves from `ERR_EXECUTION_FAILED` to a no-output success;
  - a 205 carrying a JSON payload, which the previous code decoded and returned as output, now fails.

  The README, CHANGELOG and portability record describe this as an HTTP correction plus an experimental capability extension under the retained private r1 labels, and they explicitly do not claim strict behavioural compatibility. That is an accurate and acceptable record for unpublished experiments.
- **C3: corrected.** My example wrongly implied that an undeclared 201 with JSON under a content-less `default` previously returned its value. In fact the previous adapter refused any operation with a content-less `default` as a whole. This is newly admitted behaviour, not a changed mapping for a previously admitted operation.

  The policy distinction still stands. "A selected content-less declaration requires empty bytes" is this adapter's policy, not an OpenAPI guarantee, and it can fail after dispatch. The README and `portability.md` now say exactly that, and they keep it out of generic roles and future kinds unless deliberately adopted.
- **New tests (read, not run):**
  - every dispatched call in `tests/crud.rs` now asserts the actual wire body bytes against the caller's exact body text, or absence. That covers DELETE with no body on 3.0 and `null` on 3.1;
  - a new case keeps a usable PUT represented and invocable beside an unsupported HEAD neighbour (one represented, one implementation-unsupported, not strict-eligible);
  - inspection still fails at `result_bytes = 1`.
- **Wording nit (non-blocking).** The CHANGELOG says "a payload on a selected content-less success fails instead of being returned as JSON". That is accurate for 205; 204 already failed before. But it could be read as applying to other content-less declarations whose operations were previously unsupported. Consider "…fails; previously such operations were refused, except HTTP 205 payloads, which were returned as JSON."

**Final disposition: accepted** for the reviewed code, tests and documentation, with no open material finding. This acceptance does not depend on, and does not assert, the qualification results that are still underway.
