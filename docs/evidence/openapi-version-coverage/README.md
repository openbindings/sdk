# OpenAPI 3.0 / 3.1 adapter checkpoint

Status: implementation and independent Astra/Claude reviews complete within the
stated profile. Claude's one material finding was repaired and independently verified;
refreshed affected/package/native/Node-Wasm qualification is green. Refreshed CPU budgets also passed for both editions. This is a bounded experimental profile, not complete OpenAPI support
or published binding specifications. No merge/publication is authorized by this slice.

## Result and boundaries

One `OpenApi<T>` implements source inspection, interface synthesis and binding
invocation for two private kinds. The generic operation facade remains unchanged.
The original 3.1 r1 identifiers and caller mappings retain their meaning. The new
3.0 kind has its own context/status identifiers. Core, published roles, binding
specification repositories and the pinned independent client are unchanged.

The main source fixtures are 3.0.4 and 3.1.2. The qualified profile continues to use
GET/POST, scalar path/query arguments, finite JSON, complete responses, absolute
servers and caller-supplied resources/transport. Two older 3.0 patches have only
shared-subset smoke coverage. No broad version-conformance claim is made.

The five new source/manual/oracle files were authored and hashed before the runtime
extension. Their hashes remain unchanged; only the loopback server origin is
substituted in the harness. Generated documents are tested against these same
literal wire and caller expectations, not against the adapter's generated output.

## What the second family taught us

- The kind must participate in prepared-source identity; changing the kind on the
  same source-content owner cannot bypass version applicability.
- A 3.0 parameter Schema Reference Object cannot derive its type from an ignored
  sibling. Nullable 3.0 scalar parameters also exceed the current non-null mapping.
  Both are honestly unsupported; they are not declared invalid OpenAPI features.
- Protocol references resolve through the existing client. Headerless supplied
  fragments need explicit client interpretation roots in this implementation.
  This is resource admission policy, not a mandated binding-spec mechanism.
- OpenAPI 3.0 GET request-body declarations are ignored, while 3.1 declarations remain
  effective. The client already implements this distinction; a paired public
  consumer regression now checks generated schema, literal entity bytes, refusal
  and dispatch count. See the [3.0 operation rules](https://spec.openapis.org/oas/v3.0.4.html#operation-object)
  and [3.1 operation rules](https://spec.openapis.org/oas/v3.1.2.html#operation-object).
- A nullable body remains an exact JSON value, with absence distinct from null.
  Body schemas are not translated or advertised as complete caller contracts.

The [portability review](portability-review.json) classifies the nine previous
observations and adds version applicability. These are candidate shared meanings.
They do not prescribe caches, resource-layout APIs, schema generators, internal
algorithms, languages, executor choices or repository arrangements. No previous
binding draft or derivative expectation was used as design/acceptance authority.

## Verification and reviews

The maintained tests are in `crates/openbindings-openapi/tests/versions.rs`, using
`tests/fixtures/v30`. Coverage includes kind/version/cache isolation across all roles,
manual and synthesized/reloaded HTTP calls, external references, exact body bytes,
path/query homonyms, nullable/reference boundaries, context/status isolation,
cardinality, resource limits and malformed/truncated responses.

Current completed checks:

- 128 affected SDK tests and doctests, including 31 adapter tests after the Claude repair.
- Strict adapter and external-consumer Clippy, formatting and diff checks.
- Extracted Cargo packages with the unchanged exact client Git pin.
- Minimal invocation-only consumer and core dependency isolation.
- Native and Node/Wasm lifecycle coverage for both kinds; Node completes 1,000
  mixed cycles per kind and returns to the same logical-owner baseline (SDK 8,
  client 0). These are ownership counters, not allocation measurements.

The external package consumer and Node/Wasm probe now take an explicit kind and
exercise both editions. They preserve JSON text across the JavaScript boundary.
CPU measurements use the existing registered gates separately for each edition;
the comparison is adapter versus direct-client/core work, not a controlled pre/post
regression measurement or whole-network HTTP parity claim.

Two independent GPT-6 Astra xhigh reviewers found no remaining material issues
within their respective scopes. Both inspected closure of the GET-body validation
gap. [Review reconciliation](astra-reviews.json) is a coordinator summary, not a
verbatim transcript or a claim that reviewers independently ran the test suite.

The retried [Claude Opus 5.5 Extra review](claude-review.md) identified one P2
correctness issue: raw 3.1 parameter types were trusted without checking the schema
dialect. A nine-row regression reproduced it and now verifies the dialect guard,
local overrides, honest unsupported coverage and zero dispatch on refusal.
Claude's [focused verification](claude-repair-review.md) confirms the repair closes
F1 with no new material issue. Neither Claude nor the Astra reviewers independently
ran the suite; coordinator-run evidence is separate.

The [reconciliation](claude-reconciliation.md) records the disposition of every
finding. The cache remains single-entry, and CPU budgets remain permissive: warm
single-source results cannot be generalized to alternating sources or concurrent
preparation. Node error-path tests do not independently prove the native zero-effect
guarantees. These limitations are explicit, not hidden by a pass count.

Qualification retained two failed setup runs: the new loopback harness inherited
nonblocking socket state on macOS, and new example JSON was missing a closing brace.
Both setup errors were corrected; neither required product-runtime changes.

## CPU result

Both the original run and the post-repair refresh passed all 22 registered gates
per edition: **44 pass, 0 fail, 0 inconclusive** in each run, with no timing retries. Three fresh alternating process pairs and seven samples
per row were retained for each edition. Each gate checks every process pair.
The original archive receipt and tested-input hashes match the original measurement runs.
The repair has a separate receipt and fresh measurements; historical evidence is retained.

Post-repair warm binding results (microseconds; programmable delivery, no network
latency; a repeatedly used single source):

| Edition | Method | Fixture | Direct µs | Adapter µs | Ratio | Paired overhead µs |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| 3.1.2 | GET | 1 KiB | 5.11 | 8.91 | 1.74× | 3.80 |
| 3.1.2 | GET | 64 KiB | 161.84 | 209.29 | 1.29× | 47.31 |
| 3.1.2 | POST | 1 KiB | 5.71 | 12.27 | 2.15× | 6.61 |
| 3.1.2 | POST | 64 KiB | 161.57 | 363.32 | 2.25× | 202.37 |
| 3.0.4 | GET | 1 KiB | 5.06 | 8.89 | 1.76× | 3.82 |
| 3.0.4 | GET | 64 KiB | 161.30 | 209.87 | 1.30× | 47.87 |
| 3.0.4 | POST | 1 KiB | 5.71 | 12.31 | 2.16× | 6.62 |
| 3.0.4 | POST | 64 KiB | 161.21 | 359.42 | 2.23× | 198.21 |

The GET fixture size is the response size; POST uses that size for both request and
response. Gate accounting includes the input envelope plus output bytes. Paired
medians need not equal the difference of separately aggregated medians. The current raw
observations and summaries are in `claude-repair/performance/`; original measurements
and the unchanged runner/measurement source are retained in `performance/`.

The registered ceilings permit substantial overhead (roughly 6–11× the direct cost
at the original measured invocation sizes). These are broad budgets. The measured
ratios and paired overheads above are more informative than the pass count. Passing
these gates does not establish zero regression from the previous commit, competitive
whole-network performance, or alternating-source performance.

The [summary](summary.json) and [current qualification receipt](claude-repair/qualification/receipt.json)
record the repair's results and tested-input hashes. The [original receipt](qualification/receipt.json)
remains historical evidence for the pre-repair CPU runs. The [plan](plan.md) is the frozen starting
assignment; this report records its outcome. The initial red test and both failed
setup runs remain in `failures/`.

## Continuing limitations

Literal Unicode paths, general source-schema translation, additional parameter/media
forms, authentication, streaming, automatic source acquisition, browser/Worker
networking and a published TypeScript adapter API remain outside this checkpoint.
Logical-owner cleanup and existing structural copy/retention analysis do not measure
allocator peaks, JavaScript heap or RSS. The client revision remains
`fe09522a9b99a6c6b4918e3de0467b5a638afa5e`.
