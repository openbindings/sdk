> Frozen pre-implementation plan, copied from the workspace design record. Its original status statements and proposed choices describe planning time. See [execution report](README.md) and [case results](cases.json) for executed status. Baseline repository paths resolve from `/Users/matt/Code/ob-pj`, not this copied directory. The original planning checker remains in the workspace design record.

# Authenticated JSON operations: bounded adapter slice

Status: designed on 2026-10-13; implementation and runtime qualification have not
started. Start from SDK `abc96027691187ad11fe0394f1d1c9d73482e093` on a new branch
`codex/sdk-openapi-auth-json`. Keep the independent client pinned to
`fe09522a9b99a6c6b4918e3de0467b5a638afa5e`. The previous slice remains closed.

The deliverable is one optional OpenAPI provider that can inspect, synthesize,
save/reload, and invoke a finite secured JSON operation through the existing
binding and operation invokers. Applications supply credentials for each attempt.
A developer should not need a separate authentication invoker or a new SDK role.

This document sets engineering acceptance criteria for this implementation. It is
not a binding specification or an expansion of the published role interfaces.
The concrete context design below is a proposed private experiment; Gate A freezes
it against independent consumer examples before product edits begin.

## Authority and exclusions

Use only the exact allowed inputs in [baseline.json](baseline.json): OpenBindings
core, the adopted role definitions, the pinned current SDK/client, applicable
upstream OpenAPI/HTTP rules, and the consumer needs in this plan. Implementation
facts do not become semantic authority. In particular, do not treat the client's
selection indexes, current errors, or internal resolution/cache APIs as requirements
for a binding kind.

**Do not read, quote, use as a comparator, or derive expected results from any old
OpenAPI binding draft or its derivative plans, fixtures, semantic decisions or
review conclusions.** Follow no links into those materials. Current fresh private
experiment behavior is a compatibility baseline, not a normative binding draft.
Reviewers receive this allowlist and fresh cases, not whole-repository research tasks.

Applicable upstream rules are linked in the baseline. Security requirements can
combine schemes and offer alternatives; operation declarations can replace global
security, including anonymous access. These meanings come from the source version,
not this adapter's preferred selection procedure.

## Scope

| In this slice | Outside this slice |
| --- | --- |
| Existing private 3.0 and 3.1 kinds; primary fixtures 3.0.4 and 3.1.2 | Another version family or full-version conformance claims |
| HTTP bearer and API keys in declared headers | Basic, OAuth2/OIDC schemes, mTLS, API keys in query/cookies |
| Already obtained, per-attempt credential strings | Login, issuance, refresh, verification of token claims/roles, a credential broker/store |
| Required combinations and alternatives of supported schemes; anonymous alternatives and operation overrides | Dropping unsupported alternatives silently, choosing from credential availability, authentication negotiation/retry |
| Existing GET/POST scalar path/query and finite JSON profile | New methods, parameter/media forms, streaming or schema translation |
| Manual and synthesized documents, cold reload with supplied resources | Automatic resource acquisition or embedding a full resource graph |
| Rust plus the existing exact-text local Node/Wasm seam | Published TypeScript adapter package, browser/Worker networking or CORS qualification |
| Targeted auth costs, bounds, cleanup and unaffected-path regression checks | Multi-source cache redesign, general allocator/browser-memory qualification |

Only empty scope/role arrays are admitted for these two mechanisms initially.
Nonempty role arrays in 3.1 are an implementation limitation, not invalid OpenAPI;
3.0's non-OAuth scope constraints are evaluated under its own upstream rules.
`bearerFormat` remains descriptive; it is not permission to decode or validate JWTs.
Unsupported mechanisms remain known unsupported targets with no generated callable
operation. Initially an operation with any unsupported effective alternative is
unsupported as a whole. That conservative policy avoids presenting a narrowed set
as full support. Unrelated supported operations remain usable.

## Responsibility boundaries

- **Core and generic roles:** unchanged document model, framing, context forwarding,
  challenge shape, lifecycle and terminal meanings. No OpenAPI-specific credential
  lookup, token store or source acquisition enters these crates.
- **Adapter:** supported-profile analysis; private selection/context mapping;
  conversion of selected caller credentials to client inputs; honest synthesis
  coverage and actionable challenges. No independent HTTP auth encoder.
- **OpenAPI client:** effective source declarations, references and source-version
  interpretation; request preparation, header application, collisions and final
  request-origin checks using explicitly supplied credential scopes.
- **Application/host:** credential acquisition, trust and release decisions, transport
  policy/TLS, storage and logging. A source declaration or challenge never grants
  authority to release a credential. Custom hosts remain trusted application code.

The client already exposes `SecurityAlternative`, `SecurityRequirement`,
`SecurityScheme`, `Input.credentials`, `Credential.origins`, and `Selection.security`.
These are capabilities to reuse, not proof that every needed behavior is qualified.
If a client defect blocks a required case, preserve a minimal reproducer and record
an explicit client dependency. Do not copy client logic into the adapter or silently
change the pin. Unrelated cases can continue; the slice cannot be called complete.

## Proposed consumer contract

The role already defines `context.credentials[name]` as a string for named
`auth.bearer` and `auth.apiKey` requirements. Use that as the documented primary path.
The operation input remains exactly the current parameters/body envelope.
Credentials never become operation-input fields or authored source/binding content.

The initial experiment adds one private configuration point per kind:
`local.openapi-json-probe.r1.security` and
`local.openapi30-json-probe.r1.security`. Its value contains:

- `schemes`: the exact set of declared scheme names selecting one effective security
  alternative, represented as a duplicate-free string array; order is immaterial.
  `[]` selects an explicitly available anonymous alternative. It is optional when
  there is only one distinct alternative. Identical duplicate alternatives may map
  to the same semantic choice. A selector is not a client array index.
- `origins`: a map from each selected scheme name to caller-approved origin URLs.
  The client compares scheme, host and effective port canonically. Scope is checked
  against the actual request destination, not the document's retrieval URI.
  Never populate grants automatically from a source, challenge or selected server.

See the candidate [context](plan-examples/context.json),
[missing-credential challenge](plan-examples/missing-credential-challenge.json), and
[alternative-selection challenge](plan-examples/alternative-selection-challenge.json).
All strings called credentials in these files are manufactured test sentinels.
These examples are proposals, not claims about a currently implemented API.

Use the existing role's convenience forms only as compatibility inputs:
`bearerToken`, `apiKey`, and `apiKeys[name]`. A flat form can resolve only one
unambiguous selected requirement of its family. Conflicting applicable named,
legacy or flat values refuse; equal values may coalesce. No precedence rule may
silently choose between different secrets. Do not add new convenience spellings.
Unused credentials are not forwarded or used to select an alternative. Existing
unsecured operations continue to omit authentication even if context contains keys.

The private `origins` map is the proposed portable JSON seam for this adapter's
release-scope policy, chosen so Rust and a future TypeScript API can share one
context path. Gate A compares it with a typed host-owned grant seam using the same
consumer needs, then records one choice. Do not implement both to avoid deciding.
If a future binding specification adopts a configuration point, that point's
observable meaning would need definition; adopting this grant mechanism at all
is not presumed. No origin-policy algorithm is added to a generic interface.

### Selection and challenge behavior

There is no first-alternative, available-credential, or failure-driven selection.
A single distinct alternative may be used without a selector. Multiple alternatives,
including anonymous versus authenticated, require an explicit semantic selection.
A valid selected combination applies all its requirements together. No fallback to
another alternative is attempted on failure. Origins do not select security schemes.

Server selection retains its current point and behavior. If destination choice is
missing, challenge for it before asking the application to release credentials;
then a fresh attempt can challenge for security choice, missing credentials or
missing grants. Avoid a destination-by-security Cartesian-product challenge.
For an authenticated branch, use the existing `auth.bearer`/`auth.apiKey` requirement
families with source-declared names. Use `config.value` for private selector/grant
values; RFC 6901 paths address within that point. Selection schemas describe sets
without imposing array order. Grant requirements identify the selected origin in
nonsecret explanatory metadata/examples, without treating that assertion as permission. An anonymous selection branch
still has its nonempty selector requirement, satisfying the adopted role shape.

Challenges describe the context still needed for a particular attempt. They never
include supplied secret values, store credentials, spend approval, start a host,
resume the same completed attempt or promise that other caller inputs are valid.
Do not invent a global malformed-input/error precedence rule for the interfaces.
The challenge target remains an untrusted scope assertion; include the relevant
kind, operation, destination and public security requirements, without secret data.
Exact encoding/digest strategy is an implementation choice, not a persistence contract.

Well-formed absent context can produce `CONTEXT_REQUIRED`. Malformed credentials,
conflicts, an invalid selector, an explicit grant for the wrong origin, unsupported
profiles or exhausted admission budgets refuse with no dispatch. Do not ask for a
broader grant when the caller has explicitly supplied a nonmatching one. Supplying
corrected context means starting a deliberate new invocation. After host entry,
401/403 use the existing kind-specific status failure; never turn them into a
no-effect challenge or silently refresh/retry. Existing cancellation/drop semantics
and the distinction between output and terminal success remain unchanged.

No specific preflight implementation is required in this slice. The existing
advisory default remains valid and must not be documented as auth readiness proof.

## Inspection and synthesis

Inspection remains source-only; it never needs credential material. A supported
secured target receives the same sound mechanical operation framing as an
unsecured target. Missing runtime credentials are not implementation-unsupported.

Synthesis preserves the supplied source and the existing path/method binding
locator, leaves credentials out of the OBI, and describes runtime prerequisites
using existing coverage fields. A flat list must not misrepresent OR branches as
one AND; use nonsecret structured coverage details when that distinction matters.
That detail is explanatory output, not a second source of invocation authority.
Do not add auth schema properties to the operation's ordinary input contract.
Runtime prerequisites do not by themselves imply schema loss; preexisting schema
projection loss continues to be reported independently.

A cold reloaded document, independently supplied resources, a new provider and new
per-attempt credentials must reproduce the manual consumer's wire exchanges.
Generated operation keys must remain freely renameable without changing selection.
No credential value or caller-supplied grant is stored in the prepared-source cache.
A caller deliberately resupplying context on a later call is new caller input,
not a cache-isolation violation. No cross-source authorization or cryptographic
identity guarantee is inferred from a shared scheme name or an opaque target string.

## Fixtures and execution sequence

[Cases](cases.json) are required outcomes, currently all unexecuted.
[Decisions](decisions.json) separate proposed choices from portable meanings.
[Performance](performance.md) fixes measurement claims and engineering budgets.
The [seed manifest](plan-fixtures/manifest.json) describes fresh upstream documents and
literal request expectations authored for this plan. None came from a binding draft.
The seed oracle is independent of adapter output; it is not the entire required corpus.

1. **Gate A — freeze the experiment.** Verify pins and clean branch setup. Run small
   direct-client probes for effective security/selection, origin checks and relevant
   references; those probes establish client capability, not expected authority.
   Compare the two context/grant designs. Freeze one contract, the fresh manual OBI,
   all required source variations, caller outcomes and literal HTTP oracle before
   runtime edits. Hash the frozen files and record provenance. No success expectation
   may be produced by serializing a candidate request or synthesized document.
2. **Gate B — one complete bearer consumer.** On 3.1.2, implement inspect → manual
   invocation → synthesis → save/reload → invocation. Exercise a missing-credential
   challenge followed by a new authorized attempt, exact header/body values and
   origin rejection. One provider and the existing public invokers throughout.
3. **Gate C — close the support matrix.** Add header API keys, combinations,
   alternatives, anonymous and inherited/overridden requirements, supplied references,
   both editions and honest exclusions. Retain anonymous behavior and the dialect
   fix. Finish secret-handling, bounds, cancellation and adversarial-context cases.
4. **Gate D — qualify consumers and costs.** Run affected tests/Clippy, extracted
   packages, core/minimal-consumer isolation, real loopback HTTP with manufactured
   values, and local Node/Wasm consumers. Use programmable HTTPS destinations for
   scheme/origin cases; loopback HTTP is a synthetic transport fixture and does not
   establish secure production transport. Run registered measurements once, preserving
   every failed or inconclusive result. Add explicit Node effect-count assertions
   on error paths rather than repeating the previous probe's evidentiary gap.
5. **Gate E — independent closeout.** Request two bounded GPT-6 Astra xhigh reviews
   (source/portability semantics; consumer/effects/resource behavior) and a Claude
   Opus 5.5 Extra review. Give them the allowlist, fresh contract/cases and code.
   Keep their initial conclusions independent. Reconcile concrete findings, make at
   most two focused repair passes per material finding, and have the affected repair
   independently checked. Re-run only checks changed by the repair.

Reviewers can run in parallel after the coherent implementation is ready; contract
choice, manual-oracle freezing and dependent implementation gates stay sequential.
Do not spawn separate implementers to invent competing public APIs. Execution of
this plan is a subsequent action; this planning task dispatches no reviewers.

## Bounds and exit criteria

Add at most one necessary adapter context-byte option (proposed default 64 KiB);
all auth traversal/copying and challenge construction must have explicit logical
bounds. Reuse client `document_bytes`, `nodes`, `reference_steps`, `descriptor_work`,
`headers`, SDK `max_error_bytes`, and adapter `result_bytes` where applicable.
Record exactly which fields bound each cost before coding. Charge retained security
metadata. If an existing bound does not cover a cost, add a small adapter-owned
bound rather than claim an unrelated limit protects it. Context byte counts do not
bound a caller's already-retained JSON arena or guarantee zeroization.

Exit requires all of the following:

- Every required AJ case passes for its declared editions and host scope. Every
  negative case asserts effects/output/terminal outcomes, not merely any error.
- Manual and synthesized/reloaded consumers match independent request and caller
  oracles through binding and generic operation invocation. No hidden prerequisite
  on a prior inspection, warm cache, credential store or generated operation name.
- Credential values are absent from authored documents, coverage, challenges,
  SDK-generated error/debug text, cache state and maintained logs. Successful remote
  output is still exact application data, not a credential-redaction service; a
  custom host or caller can explicitly inspect its own supplied/request values.
- Only the chosen schemes' credentials reach the chosen destination. Missing,
  malformed, conflicted or out-of-scope context gives zero host entries; multiple
  input values, cancellation and post-dispatch failures retain existing guarantees.
- Bounds pass exact-limit/one-over cases; same-source warm calls do not re-analyze
  source because credentials changed; owner/host registrations return to baseline.
- Extracted Rust and local Node/Wasm consumers pass. No auth model is pushed into
  core or generic roles. No new public invocation facade, token store or broker.
- Frozen performance gates pass with reported absolute costs, ratios, incremental
  auth overhead and raw receipts. Inconclusive timing cannot be described as passing.
- Independent review has no unresolved material correctness, credential-isolation,
  boundary or API-coherence finding within this scope. Required reviewer absence
  leaves a review-pending checkpoint rather than an invented concurrence.
- The decision ledger classifies every consequential choice as upstream/adopted
  meaning, candidate portable meaning, implementation policy, or unresolved evidence.
  There is no obligation to write a binding spec; no old draft is rehabilitated.

A remaining client/role blocker or a stable failed gate yields an explicit partial
checkpoint with reproducer and remaining work after the bounded repairs. Do not
weaken scope or rewrite an oracle to obtain green checks. Unrelated broad capabilities,
cache optimization, URI-normalization coverage and production distribution remain
follow-ups. A substantive scope change returns to design, not an unbounded loop.

Save maintained consumer/tests, a concise execution report, exact tested-input
hashes, raw qualification/measurement receipts and review reconciliations in the SDK
when implementing. Commit on the task branch; this assignment does not authorize
merge, publication, or changes to the website, role publications or binding specs.

Planning check: `python3 authenticated-json/verify-plan.py` from the loop root.
It checks plan consistency/pins only; it does not execute or qualify the adapter.
