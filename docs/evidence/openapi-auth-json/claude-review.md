# Independent review: authenticated JSON adapter slice

Reviewer: Claude Opus 5.5 (Extra), 2026-10-13. Subject: working changes on `codex/sdk-openapi-auth-json` relative to `abc96027…`, as reproduced in `claude-input.txt`.

## Scope and limitations

**Read:**
- the packet: Gate A, the plan, `security.rs`, `analysis.rs`, `invocation.rs`, `synthesis.rs`, `lib.rs` and the consumer `auth.rs`;
- the pinned client at `fe09522`, through `git show`: `prepare.rs` (`apply_security`, `describe_security`, `security_descriptions`, final header validation) and `transport.rs` (`Header::validate`);
- the synthesizer role's `requirements` and `details` text at the interfaces revision `f158f4f` named in `baseline.json`.

**Not read:** fixture JSON outside the packet, other reviews, and any binding draft or derivative.

I ran no tests. Results attributed to tests are the coordinator's. Package, Node/Wasm and CPU qualification are still pending, so the slice is not complete.

Line references are to the packet's file sections. Findings are **verified (static)** when they follow from code read in both the adapter and the pinned client, and **evidence gap** when a test or receipt does not establish a claim.

## Verdict

- **Credential isolation and effect boundaries are sound.** I found no credential leak and no refusal after host entry.
- **The design boundaries are coherent.**
- **One narrow coverage-truthfulness blocker (A1)** should be fixed before exit.
- **Two minor admission and coherence repairs (A2, A3)** are recommended.

## Findings

### A1 (P2, blocker for honest coverage). Alternatives that can never be satisfied are represented as callable (verified, static)

**Location:** `analysis.rs` 245–269, where `profile()` accepts any alternative made only of bearer schemes and header API keys.

**Why it fails:** the pinned client detects header collisions only when it applies credentials. `apply_security` returns `CredentialCollision` when a header name is already present, compared case-insensitively. The final `Header::validate` rejects a name that is not a valid HTTP token. `describe_security` (prepare.rs 1476–1491) validates neither.

**Triggering sources,** each upstream-declarable:
- one alternative with two `http`/`bearer` schemes, so two `Authorization` fields;
- one alternative with a bearer scheme plus an `apiKey` in a header named `authorization`. This is the suite's `collision` variant, with schemes `["ServiceToken","Other"]`;
- two header API keys whose names differ only in case (`key-collision`);
- an `apiKey` header whose name is not a token, for example `"X Partner Key"`;
- an `apiKey` header named `Content-Type` on an operation with a body. This one fails whenever a body is sent.

**Expected:** under the plan's conservative rule ("an operation with any unsupported effective alternative is unsupported as a whole"), the operation is reported as implementation-unsupported with no generated operation.

**Actual:**
- Inspection gives the operation an input-schema hint.
- Synthesis emits a represented, strict-eligible operation with security details.
- Every invocation of that alternative is refused before effects.

The suite (`auth.rs` 91–93) asserts only the refusal, never the coverage.

**Narrow repair:** in `profile()`, compute each alternative's header field names. A bearer scheme writes `authorization`; a header API key writes its lowercased `name`. Return, for example, `security-header-unusable` when:
- two names in one alternative are equal;
- a name is empty or is not a token;
- the operation declares a request body and a name equals `content-type`.

This is a name comparison, not HTTP encoding. Alternatively, record a client dependency for describe-time validation. Then add coverage assertions to the collision variants.

### A2 (P3). An empty API key is sent (verified, static)

**Location:** `security.rs` 197–207 (`consistent`) accepts `""`. Unlike the bearer path, the client's API-key path does not reject empty material, and `Header::validate` allows an empty value.

**Trigger:** `credentials.PartnerKey = ""` with a valid grant.

**Actual:** the host is entered with an empty `x-partner-key` field. A 401 would then arrive as a status failure after effects.

**Expected:** refusal with no dispatch, consistent with how an empty bearer token is treated and with the plan's "malformed credentials … refuse with no dispatch".

**Repair:** treat empty material as malformed in `consistent`, and add the case to the suite.

### A3 (P3). Challenge bounds are enforced even when no challenge is needed (verified, static)

**Location:** `security.rs` 86–97.

**What happens:** `target` and `Challenge::new` are built and charged against `result_bytes` before the code knows whether a challenge is needed. A fully specified, valid context is therefore refused whenever an empty challenge would exceed `result_bytes`. That limit is reachable with a small configured value, which the bounds tests already use for synthesis.

**Repair:** build the challenge lazily, only on the two challenge paths.

## API coherence notes (non-blocking)

- **Coverage `requirements` holds a sentence instead of names.** The role defines `requirements` as "Named runtime prerequisites"; synthesis.rs 134–141 emits one prose sentence. Every authenticated represented target needs the kind's security point, either for `schemes` or for `origins`. Naming that point would therefore be accurate and would not flatten OR into AND. Credential names can stay in the structured `details`.
- **Selection takes two round trips.** The selection challenge (`security.rs` 98–111) offers alternatives containing only the `schemes` selector, so satisfying one triggers a second challenge for credentials and grants. Adding each alternative's own credential and grant requirements is linear, not the destination × security cross product the plan avoids, and would make each alternative complete for a single retry. The plan permits the current form.
- **`context_bytes` counts the whole context.** Unrelated members, such as `metadata` or other points, count against it, so a large application context refuses only secured operations. Document this, or count only the members read.
- **Origin examples come from the source.** The grant requirement's description and `examples` contain the server origin the source declares. That is permitted by the plan, but the README should state that this is a source assertion, never a suggested grant.

## Checked without finding

**Selection:**
- Set-valued selection: sorted `BTreeSet` keys match `BTreeMap` keys. Duplicate alternatives coalesce to the first index. `[]` selects only a declared anonymous alternative.
- There is no selection by credential availability and no fallback. A single distinct alternative is auto-selected.
- The adapter's index comes from `describe().security`, which preserves the raw order that `apply_security` indexes (prepare.rs 1242–1302).

**Credential scope:**
- Only the selected alternative's credentials are inserted, and unselected or flat values never select or leak.
- Flat forms are refused when the selected alternative is ambiguous for that family. Named, `apiKeys` and flat values must agree.
- The adapter requires a grant matching the selected server's origin, and the client re-checks the final target origin. This is a double check that also covers path-induced origin changes.
- The anonymous and unsecured paths forward nothing.

**Effects:**
- Every refusal and challenge precedes `operation.prepare` or happens inside it, before host entry.
- 401, 403 and 302 become kind-specific status failures with one effect each. Cancellation and drop behaviour is unchanged.

**Secrets:**
- Challenges are built from names and source origins only.
- Client `Credential` and `Header` `Debug` output is redacted.
- Credentials never enter `Prepared`, inspection or the synthesized OBI. Query and cookie keys, which would put secrets in URLs, are excluded.

**Bounds:**
- Context traversal is bounded by `context_bytes`.
- Credential and origin bytes are also bounded by the client's `document_bytes` input accounting.
- Security metadata is charged in `retained_bytes`. Challenge serialization is checked incrementally and again at the end.

**Upstream rules:**
- Scopes in 3.0 non-OAuth requirements are rejected by the client as invalid. Role arrays in 3.1 are an implementation limitation.
- Entry-document resolution of implicit scheme names is a choice Appendix F permits, as Gate A records.
- Bearer material is checked against the RFC 6750 b64token character set by the client.

## Evidence gaps

- **Not yet available:** package, Node/Wasm and CPU receipts. Node error-path effect counts are a stated Gate D requirement.
- **No coverage assertion for collision variants** (A1).
- **No empty or whitespace API-key case** (A2).
- **No small-`result_bytes` invocation case** (A3).
- **No direct test of the selection-challenge schema.** The `uniqueItems` / `minItems` / `maxItems` set encoding and the `{"const":[]}` anonymous branch are checked only as producing `CONTEXT_REQUIRED`.
- **Secret absence is checked only on errors' `Debug` text.** It is correct by construction for challenge data, but a `to_json()` assertion would make it explicit.

## Design boundaries

They remain coherent:
- **Core and roles are untouched.** The role's `credentials[name]` path is reused as the primary carrier. Selection and grants live in one private point per kind with no new generic vocabulary.
- **Responsibilities are split cleanly.** The client owns encoding, collision handling and final origin checks. The adapter maps context to client input and reports coverage.
- **Portable meaning is not overclaimed.** The selector and grant semantics are documented adapter policy, not role or specification requirements.

**Recommendation:** fix A1, preferably together with A2 and A3, as one focused repair with the matching assertions. Then complete the pending package, Node and CPU qualification. Do not call the slice complete before both.
