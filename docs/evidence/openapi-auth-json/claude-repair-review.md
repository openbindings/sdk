# Repair verification addendum

Reviewer: Claude Opus 5.5 (Extra), 2026-10-13. Input: `claude-repair-input.txt`.

**What I read** (current working tree): `security.rs`, `analysis.rs` (security profile), `synthesis.rs`, the README's "Authenticated JSON calls" section, `examples/openapi-json-path-consumer/src/auth.rs`, `wasm.rs` and `wasm-probe.ts`. I also checked, via `git show` at `fe09522`, which header fields the pinned client writes during preparation.

**What I did not do:** I executed no tests. I deliberately avoided cargo builds so I would not contend for the target directory or perturb the separate CPU qualification. Everything below is static verification. Test results mentioned are the coordinator's or are described as present in source.

## Verdict

All three findings are closed, and I found no remaining material blocker within this slice's contract and code:
- **A1 and A3 are repaired**, as is the incremental challenge bound.
- **A2 is withdrawn**: the portability rationale is correct.

Closeout still depends on the pending package, Node/Wasm and CPU qualification receipts.

## Verified repairs (static)

### A1: statically unusable alternatives

`profile()` (`analysis.rs:245–293`) now computes each alternative's header field names: bearer writes `authorization`, and a header API key writes its own name. It then rejects:
- any name that is empty or contains a character outside the RFC 9110 `tchar` set (the set is exact);
- `content-type` when the operation has an effective request body. For a 3.0 GET, the client reports no body, so the check correctly does not fire;
- any case-insensitive duplicate within one alternative, which also covers two bearer schemes.

The whole operation is then reported implementation-unsupported, consistent with the plan's rule.

The rule is complete against the pinned client. Preparation itself adds only parameter headers (unsupported here), `Content-Type` (covered) and `Cookie` (only for cookie parameters or keys, both unsupported). Assertions are present for:
- the `collision` and `key-collision` variants: refusal, zero host entries and implementation-unsupported coverage;
- the invalid names `"X Partner Key"`, `""` and `"content-TYPE"`;
- two bearer schemes, where the inspection target carries no operation hint.

### A3: lazy challenge construction

The challenge identity and writer are now built inside a closure that is called only on the two challenge paths (`security.rs:91–113`, `175–188`). A complete, valid context no longer depends on the challenge budget.

**Remaining test gap:** no case shows success at a `result_bytes` value where a challenge would not fit. The laziness is evident from the code, so this is not blocking.

### Incremental challenge bound

`Bounded::write` (`security.rs:228–263`) refuses a fragment before appending it, so the stored bytes never exceed the limit.

`serde_json` writes fragment by fragment, so the charge is incremental. Repeated origins no longer form a `Value` tree before admission, and commas are written and charged exactly.

Every write is followed by `?`. A partially written challenge is dropped and the call returns `ERR_REFUSED`, so a truncated alternative cannot reach the caller.

Unit tests are present: an exact budget passes and one byte under it refuses, and repeated 32 KB origins stop below the limit. The facade's separate `max_error_bytes` conversion to `ERR_EXECUTION_FAILED`, with zero host entries, is tested and documented as existing generic behaviour.

### Coverage requirements and README

- `requirements` now names the kind's security point (`synthesis.rs:135`). That is accurate for every authenticated represented target, since either a selector or a grant is always needed, and the AND/OR structure stays in `details`.
- The README now documents:
  - the whole-context size limit;
  - that origin examples are untrusted;
  - the staged challenges;
  - the `max_error_bytes` behaviour;
  - the exclusions.

### Node error-path evidence

`authenticated_call` takes the raw JSON context text and returns the actual output, host-entry count and wire record. On both editions and both mechanisms, the probe asserts:
- exact success, including credential header values and exact request body text;
- zero host entries and no output for missing context, a wrong grant and an escaped CR/LF credential.

This closes the earlier gap in Node effect evidence once a receipt exists.

## A2: rationale accepted, finding withdrawn

My original A2 rested on consistency with the bearer path. That is not a valid authority:
- The role defines `auth.apiKey` material only as a string.
- The OAS API Key scheme defines a field name and location, never a value grammar.
- RFC 9110 field values may be empty.
- Bearer is different because RFC 6750 defines `b64token` with at least one character.

Applying each mechanism's own grammar is principled. A uniform non-empty rule would invent semantics the adapter has no authority for, which goes against the slice's minimal-boundary aim.

The implemented behaviour is now documented and tested:
- `""` is preserved and dispatched;
- whitespace and control characters are refused by the client's field validation with zero host entries;
- an empty bearer token is refused.

Treating an unset value as `""` is the application's responsibility.

## Non-blocking residuals

- **README wording on inspection.** README line 156 says inspection and synthesis both report prerequisites under the security point. Only synthesis coverage does; `BindableTarget` carries none. Narrow the sentence to synthesis.
- **Framing and connection header names.** API-key names such as `Host`, `Content-Length`, `Transfer-Encoding` or `Connection` pass the token check, and the client applies them. Release stays bounded by the origin grant, so this is not a cross-origin leak. Record it as a client or host follow-up for connection- and framing-level field names rather than an adapter blocker.

## Pending evidence (not verified here)

- execution receipts for the native auth suites and the Node/Wasm probe in this tree;
- extracted-package checks;
- CPU and performance gates.

**Recommendation:** with A1 and A3 repaired and A2 withdrawn on the stated grounds, close this review. The slice can be called complete once the pending qualification receipts are green and the one-line README correction is made.
