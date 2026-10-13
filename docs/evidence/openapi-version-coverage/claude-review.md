# Closeout review: OpenAPI second-version slice

Reviewer: Claude Opus 5.5 (Extra), 2026-10-13. Subject: SDK `6137585efe220f49a96fadd5af8e0504aacc38f1` against parent `45c69a9555d5a99ab3ac77ff8f1903547f399e1d`.

## Scope and limitations

I read the whole review packet. Within the brief's allowlist, I also read parts of the pinned client and reqwest companion at `fe09522` (path-item resolution, reference editions, schema dialect selection and redirect policy). I also checked two passages of upstream text, from OAS 3.0.4 and 3.1.2.

I ran no tests, builds or measurements. Every count and timing below comes from the parent run's qualification summary. Findings are labelled **verified (static)** when they follow directly from code I read, or **inference** when they depend on behavior I did not execute. No other review opinions, drafts or plans were consulted.

## Findings

### F1 (P2). 3.1 parameter types are derived without regard to the schema dialect (verified, static; not executed)

**Location:** `crates/openbindings-openapi/src/analysis.rs:276–298`, in `profile()`. For the 3.1 kind, the code reads the raw `type` member of `param.schema()`. The 3.0 guard at lines 277–291 has no 3.1 counterpart. The test row at `tests/versions.rs:280–285` asserts that `{"$ref":…,"type":"string"}` is represented under 3.1.

**Why it is wrong:** OAS 3.1.2 defines `jsonSchemaDialect` as "the default value for the $schema keyword within Schema Objects". A Schema Object may also carry its own `$schema`. The pinned client honours both when it selects a schema's profile (`client/src/schema.rs:524–530`, `prepare.rs:625–650`), and it exposes the result via `Parameter::schema_description()` (`prepare.rs:189–195`). The adapter never consults it.

Under a draft-07 or earlier dialect, the siblings of `$ref` are ignored. That is the same situation the slice correctly treats as "type not established" for 3.0. Under an unrecognized dialect, `type` is opaque.

**Trigger:** an OAS 3.1.2 document with:
- `"jsonSchemaDialect":"http://json-schema.org/draft-07/schema#"`
- `components.schemas.Count = {"type":"integer"}`
- a query parameter `limit` with schema `{"$ref":"#/components/schemas/Count","type":"string"}`

**Expected:** implementation-unsupported, with no operation emitted. This matches the established 3.0 choice.

**Actual (by reading):**
- The target is represented and strict-eligible, with `limit: {"type":"string"}`.
- `{"parameters":{"query":{"limit":5}}}` is refused before effects.
- `"limit":"abc"` is admitted and sent.

The OBI's caller contract is therefore the opposite of the description's effective schema. No unsafe effect follows: the server would answer with a status error. The defect is a false representation claim, and synthesis would carry it into a saved OBI.

**Minimal fix:** for the 3.1 kind, establish `type` only when the parameter schema's profile is the OAS 3.1 base dialect or JSON Schema 2020-12. Otherwise return `parameter-schema-dialect-unimplemented`. Add the dialect row, plus one row with a schema-level `$schema`, to `schema_dialect_boundaries_do_not_invent_a_caller_contract`.

This narrows the profile. It does not add dialect support, and it needs no specification content.

### F2 (P3). The single-entry cache re-runs whole-document analysis while holding the provider-wide lock (verified, static; cost not measured)

**Location:** `src/lib.rs:228–246`. The `cached` guard is held across `analysis::prepare`. The cache is emptied (`*cache = None`, line 240) before the fallible analysis runs. Each miss calls `operation.describe()` for every operation in the document (`analysis.rs:143–211`, with `profile()` at line 195).

**Trigger:** one `OpenApi<T>`, as the README now invites for both kinds, serving an OBI with two sources: for example, a 3.0 and a 3.1 description, each with a binding. Invoke A, B, A, B.

**Expected:** a warm call after first use, which is what the summary's warm-binding rows imply.

**Actual:**
- Every call misses. The test-only `preparations` counter rises once per call.
- Each miss re-analyses the entire description, which scales with the operation count rather than with the one operation invoked.
- A concurrent invocation of either source on another thread blocks for the whole analysis.
- A source that fails preparation also evicts the good entry.

**Why the evidence misses it:** the qualification measures only single-source warm paths and a per-provider cold path.

**Minimal fix:** either of:
- a small bounded map keyed by (kind, content) and charged to `result_bytes`, with analysis run outside the lock and the result inserted afterwards, and no eviction on failure;
- or, at least, a README statement that a provider should serve one source, plus an alternating-source measurement row.

### F3 (P3, evidence). The registered CPU gates cannot fail at the measured scales (verified, arithmetic on the summary's figures)

**Location:** `tools/measure-openapi-json-path.py:41` (`allowance = 25000 + 10*payload` ns) and line 44 (`pass` when candidate ≤ 2×baseline + allowance). For POST, `payload` is about 2×bytes, because the envelope already contains the body.

Ceilings computed from the summary's direct medians:

| Row | Direct | Ceiling | Ceiling as multiple of direct | Measured multiple |
| --- | --- | --- | --- | --- |
| GET 1 KiB | 4.81 µs | ≈45 µs | 9.3× | 1.75× |
| POST 1 KiB | 5.43 µs | ≈56 µs | 10.4× | 2.14× |
| GET 64 KiB | 153.5 µs | ≈987 µs | 6.4× | 1.29× |
| POST 64 KiB | 153.15 µs | ≈1,642 µs | 10.7× | 2.23× |

The per-byte allowance (10 ns/byte) is roughly four times the entire direct per-byte cost (about 2.3 ns/byte). So "44 pass, 0 fail" shows only that nothing is grossly pathological.

The measured medians are the real evidence, and they are acceptable. POST 64 KiB carries a 188 µs paired overhead, consistent with re-parsing the request body (`invocation.rs:298–302`) and the response (`invocation.rs:360`), plus the duplicate-name walk (line 225).

**Fix:** in the closeout record, cite the ratios and paired overheads rather than the pass counts, or derive the per-byte allowance from the baseline. No code change is needed for this slice.

## Non-blocking notes

- **Wasm probe evidence is weaker than the native evidence.** `wasm.rs:45–46` returns before `assert_eq!(calls.get(), 1)` on any error. So the Node challenge case (`wasm-probe.ts:18`) never shows zero host calls, and the malformed case (line 14) accepts any error code. The native tests do cover both points. The Node claim should say it establishes the exact-text and ownership seam, not the effect boundary.
- **The README's description of error responses is broader than the code.** The README says error-response bodies and headers are "disclosed in coverage". In the code, `default` is admitted as a possible success profile (`analysis.rs:330`), so a content-less or `application/problem+json` `default` makes the operation unsupported rather than lossy. That conservatism is defensible, because `default` can cover a 2xx, but the README should say so.
- **Two small mismatches.** The 3.1-only `webhooks` check also runs for 3.0 sources (`analysis.rs:137–139`). It is harmless: it only marks the inventory non-exhaustive. Separately, an edition mismatch maps to `UnreadableSource` (`lib.rs:144`), where `InvalidInput` would describe a kind/content mismatch more accurately.

## Checked without finding

- **Kind separation.** The cache is keyed by kind and content. Each kind checks its own edition and uses its own status code and context point. A source with the wrong kind is refused with zero dispatches. The other kind's context point is ignored. All of this is covered by tests.
- **Mixed editions across supplied resources.** These cannot leak between kinds: the client rejects a reference target in another edition (`ReferencedEditionMismatch`, `graph.rs:1700–1702`), and resources are provider-level.
- **Path Item ambiguity.** The client fails on overlapping shared fields and attaches method conflicts. `describe()` surfaces recorded path errors (`prepare.rs:1622`), so ambiguous or unresolved mounts become implementation-unsupported. The adapter's own check only adds a clearer reason code.
- **3.0 edition semantics.** I confirmed the GET request-body rule against OAS 3.0.4 ("requestBody SHALL be ignored by consumers"). Reference Object siblings, nullable parameters and Schema Reference Objects all refuse honestly. The paired GET test checks literal wire bytes and the number of dispatches.
- **Effect boundary.** It is unchanged: no refusal or context challenge occurs after the transport is entered, and cancellation and drop behave as before. Duplicate names are refused anywhere in the input. The client enforces required bodies (the test with `{}` shows zero calls). `integer()` now handles `10.0e-1`.
- **Response mapping.** Explicit non-204 no-content success declarations are unsupported. Error-response media for explicit non-2xx codes becomes projection loss, and the failure tests now assert that no output precedes an error.
- **Redirects and proxies.** The reqwest companion sets `redirect::Policy::none()` and `.no_proxy()` (`reqwest/src/lib.rs:112,115`), which closes my earlier open question about redirects.
- **Context challenge.** The `target` is now scoped by kind, retrieval URI, path, method and choices.

## Specification implications

None. F1 is resolved by narrowing what this implementation admits. Dialect-aware type derivation is implementation policy, not binding-specification content.

## Closeout recommendation

**Close the bounded slice after the small F1 fix and its test rows.** If Matthew prefers no further code in this slice, close it with F1 recorded as an explicit limitation in the README and the coverage reason codes.

F2 and F3 do not block closeout. Record F2 as a known single-source cache limitation, and F3 as an evidence caveat, with the closeout citing the measured ratios instead of the gate counts.
