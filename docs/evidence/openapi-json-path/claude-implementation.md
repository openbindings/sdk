# r1 implementation review

Reviewer: Claude Opus 5.5, 2026-10-12. Input: `claude-implementation-input.txt` only. No code was run. The dynamic client and its reqwest companion are not in the packet, so findings that depend on them are marked **verify**. Package, Wasm and performance qualification were not assessed. Line numbers are mapped from the packet to each source file.

Result: eight actionable findings. Findings 1, 2, 3, 6 and 8 are confirmed from packet code. Findings 4, 5 and 7 are effect risks that the evidence does not pin.

## 1. Declared no-content success other than 204 is represented, then fails after effects (confirmed)

`analysis.rs` `profile()` (lines 275–292) admits a 2xx response declared without media. `invocation.rs` `accept()` (lines 325–332) gives zero-output success only to 204 and otherwise requires decodable JSON.

Reproduction: declare `post /parcels` with `responses: {"201": {"description": "Created"}}` and have the server return `201` with `Content-Length: 0`. Synthesis reports the target as Represented and strict-eligible. Every successful call ends in ERR_EXECUTION_FAILED after the POST took effect, which invites a caller retry that duplicates the effect. The same happens for 202 and for a 200 declared without content.

Minimal fix within r1: in `profile()`, return `response-no-content-unimplemented` for an explicit 2xx or `2XX` declaration with no media whose code is not 204. Accepting an empty body for a declared no-content status would need r2. Add both the synthesis case and the wire case.

## 2. Error-response declarations block operations that r1 fully handles (confirmed)

The same loop rejects non-JSON media and declared headers on every response, including non-2xx responses. r1 maps every complete non-2xx response to `{status}` and makes no claim about its body, so error media and headers never reach the caller.

Reproduction: add `"404": {"description": "Missing", "content": {"application/problem+json": {}}}`, or a `429` declaring `Retry-After`, to the lookup operation. Synthesis then marks the target implementation-unsupported, and `prepare_request` refuses a hand-authored binding before dispatch. Yet the existing 404 wire test shows the operation works.

Minimal fix: apply the media and header admission checks only to 2xx, `2XX` and `default` declarations. For non-2xx declarations, set `schema_loss` rather than failing the profile.

## 3. Ambiguous path-item merges go undetected, and skipped units disappear from an "exhaustive" inventory (confirmed in the adapter; the client's merge behavior is not shown)

In `analysis.rs` `prepare()`, the chain check (lines 145–152) only asks whether any chain member defines the method. Suppose a local `{"$ref": "...", "get": A}` points at an item that also has `get: B`. The check passes, and whichever operation the client's `describe()` picks is represented and invoked. The contract lists ambiguous path-item merges as unsupported and requires that they be reported.

Separately, four `continue` statements (lines 128–134 and 145–153) drop a unit without a coverage entry and leave `exhaustive` true. They fire for a method outside the eight, a path key without a leading `/`, a method that is not an object, or a duplicate pair. Strict synthesis can then claim completeness.

Minimal fix: if more than one chain member defines the method, or if `parameters` or `servers` overlap across the chain, set the profile error to `ambiguous-path-item-merge`. On every skip, either mark the inventory non-exhaustive or emit an `invalid` entry. Add a merge fixture.

## 4. Real-HTTP redirect behavior is not pinned (verify)

`manual.rs` line 148 uses `dynamic_openapi_client_reqwest::Client::new()`. The only redirect case, the 302 row in `semantics.rs` (line 122), runs on the programmable host. reqwest's default policy follows up to ten redirects and rewrites POST to GET on 301, 302 and 303. If the companion keeps that default, the native path sends a hidden second request and can emit the redirected 200 as success. That contradicts FR15 and the r1 http-status mapping.

Reproduction: have the wire server reply `302` with `Location: /api/parcels/box-8`, then serve that path. Assert exactly one accepted connection and an error carrying `{"status":302}`. If the test fails, the fix belongs in the companion: `redirect::Policy::none()`.

## 5. The adapter does not enforce required parameters or required bodies (verify)

`invocation.rs` `prepare_request()` (lines 183–299) refuses unknown names, duplicates and type mismatches. It never checks `required()` on parameters or `request_body.required`. The evaluator is optional, and `manual.rs` builds its invoker without one. So whether `{}` is refused before effects, for an operation with a required query parameter or `requestBody.required: true`, depends on the client's preparation step, which the packet does not show. If the client sends that request anyway, a POST the description forbids leaves the host.

Reproduction: a manual binding with no evaluator, a required body and input `{}`. Assert ERR_REFUSED and zero host calls. Minimal fix: check the declared `required` flags in `prepare_request` before `operation.prepare`.

## 6. The destination challenge's `target` cannot be scoped (confirmed)

`invocation.rs` line 295 sets `target` to `EXPERIMENTAL_KIND`. Every API and every operation therefore produces the same target and the same point, even though effective server sets are per operation. A caller that keys its selections by target, as the role allows, will offer API A's choice to API B, or one operation's choice to another, and loop through repeated challenges. Membership checking prevents wrong effects; the defect is a challenge the caller cannot act on durably.

Minimal fix: when the source carries `retrievalUri`, use it as `target`. Document that the selection applies to one invocation.

## 7. Duplicate member names inside `body` are forwarded (verify)

The envelope and parameter maps refuse duplicate names. `body` (lines 271–280) passes through `ExactJson::parse` with no such check, and the carrier test shows that `JsonValue::parse` accepts duplicates. Given `{"role":"user","role":"admin"}`, an author schema evaluated under one duplicate-resolution rule and a server applying another can disagree about the value that was validated.

Reproduction: call create with that body. Assert ERR_REFUSED and zero host calls. Minimal fix: refuse duplicate names at any depth in `body` before dispatch, matching the envelope rule.

## 8. The failure tests cannot detect an output emitted before the error (confirmed in the tests)

`semantics.rs` `invoke()` (lines 58–82) and `manual.rs` `call()` (lines 91–108) push `value?`, so an `Ok(output)` followed by an error still returns only the error code. `failed_or_incomplete_responses_never_emit_success` therefore does not test what its name says. The implementation is currently correct, because `accept()` decides before `emit`.

Minimal fix: collect every item and assert that no `Ok` precedes the terminal error. Add a 2xx case with `Content-Type: text/plain` and JSON-parsable bytes, to pin whether `response.json()` checks the media type.

## Checked without finding

- No ERR_REFUSED or CONTEXT_REQUIRED is reported after the transport is entered; not-dispatched outcomes are treated conservatively.
- Pre-close cancellation produces zero requests. Drop cancels the client token before host work is released.
- The cache mutex is not held across an await.
- Cardinality is enforced before close, and absent and null bodies stay distinct.
- Integers are checked and forwarded from exact text.
- Server selection is by exact membership, and an empty candidate set never produces a challenge.
- Kind matching is exact.
- The emitted source is preserved, generated keys are unique, and coverage entry shapes match the synthesizer role.
