# Second version slice: OpenAPI 3.0.4

Status: executing on `codex/sdk-openapi-version-coverage`, based on SDK
45c69a9555d5a99ab3ac77ff8f1903547f399e1d. Client stays pinned to
fe09522a9b99a6c6b4918e3de0467b5a638afa5e.

The deliverable is a second experimental kind in the same optional adapter and
public role APIs. The 3.1 r1 kind retains its meaning. No binding specification,
core model or published role-interface change is presumed. Existing binding
drafts and their derivative plans/tests are excluded from all inputs.

## Authority and scope

- UPSTREAM: https://spec.openapis.org/oas/v3.0.4.html and
  https://spec.openapis.org/oas/v3.1.2.html, only the source semantics exercised.
- CORE/ROLE: unchanged current core model and inspection/synthesis/invocation APIs.
- NEED: independently authored source, manual binding, expected wire exchanges
  and exact caller values, followed by synthesis/persist/reload to the same oracle.
- POLICY: the existing finite JSON GET/POST profile, explicit supplied resources,
  limits, host transport, unsupported capabilities and synthesis strategy.
- FACT: current client and SDK behavior, never normative authority.

Keep path/query scalar arguments, optional/required JSON body, complete JSON or
204 response, status-only failure and server choice. No authentication, forms,
streaming, acquisition, general schema translation or extra version family.
3.0.4 is the main fixture version; patch-family admission is distinct from full
patch-version qualification. 3.0.0/3.0.3 receive only shared-subset smoke cases.

## Frozen initial choices and cases

- V01: private `local.openapi30-json-probe.r1` source label; same carrier and
  path/method locator. Own `.server` context and `.http-status` error identifiers.
  Retain old `local.openapi-json-probe.r1` as 3.1. One provider supports both.
- V02: source kind plus complete source content participates in cache identity.
  A warmed valid source must not admit a mismatched kind or unsupported edition.
- V03: fresh 3.0.4 document plus supplied relative Path Item/Parameter resources;
  manual GET and POST compared against literal HTTP expectations. Exercise
  path/query homonyms, exact large integers, Unicode entity, absent body vs null,
  204 and HTTP failure. Independently synthesized/reloaded calls repeat the oracle.
- V04: schema meaning remains upstream. 3.0 nullable scalar parameters are outside
  this non-null scalar profile and must be disclosed as unsupported, not silently
  narrowed in synthesis. JSON nullable body is accepted without claiming body
  contract projection. Reference Objects use upstream version interpretation;
  Schema $ref translation remains outside scope. No copied OAS schema as OBI schema.
- V05: wrong kind/version, unsupported shape/media, missing resources and budgets
  produce honest coverage/refusal before host effects. Sources and extensions
  survive synthesis. Authoring emits zero operation effects.
- V06: effective server scope and context membership use each kind's point; same
  provider can alternate editions. Correct kind-specific status error on failure.
- V07: archive consumer and exact-text Node/Wasm bridge exercise both kinds,
  conditional/local host seam and cleanup. No browser networking or TS distribution
  claim. Existing invocation-only dependency isolation remains.
- V08: CPU gates unchanged from foundation/performance.md, executed for each
  family with valid source/schema shapes; all failed/inconclusive observations
  retained. No new allocations proportional to full source on cache hits. Existing
  structural versus measured resource limitations remain explicit.

## Portability review

P01–P06 need provisional shared source/locator/value/result/context/cardinality
meaning if a future binding kind chooses these mappings. Version applicability
must be unambiguous; a private identifier is a choice, not a final public name.
P07 requires no binding rule for synthesis/naming/coverage algorithms. P08 reuses
upstream semantics and records unsupported/unestablished behavior rather than
creating prohibitions. P09 keeps language, packaging, caching, executors, memory
strategy and performance policy outside binding meaning. The independent reviews
may refine this classification with concrete consumer evidence.

## Exit and review

Both families pass maintained end-to-end cases, truthful unsupported behavior and
cross-kind isolation. Public API remains one provider implementing existing roles.
No unresolved correctness issue within the claimed profile. Affected regressions,
Clippy, extracted packages, native/local-Wasm consumer, resource boundaries and
relevant CPU gates are checked. Astra and Claude reviews receive only this clean
packet and allowed current code. At most two focused repair passes per material
finding; preserve any remaining blocker. No merge or publication in this slice.
