# Ordinary JSON CRUD slice

Extend the existing private OpenAPI 3.0 and 3.1 experiments so a consumer can
inspect, synthesize, save, reload and invoke a finite JSON CRUD service. The
baseline is SDK `d975cc2` with independent client pin `fe09522`. Work belongs on
`codex/sdk-openapi-json-crud`. Old binding drafts and their expected results are
excluded. This plan does not authorize package publication or reopen AJ20.

## Scope and interpretation

Add PUT, PATCH and DELETE to GET/POST. Preserve scalar path/query inputs,
application/json bodies, explicit destination/credential context, cancellation,
finite budgets and the existing status-only error mapping. HEAD, OPTIONS, TRACE,
forms, richer parameters, response metadata, schema projection and public TS APIs
remain separate work. PATCH here carries application/json; JSON Patch and Merge
Patch media types are not silently admitted.

Use the client's effective request-body description: 3.0 ignores DELETE body
declarations; 3.1 can describe them. Test both without inventing a common rule.
The authority is the upstream [3.0 Operation Object](https://spec.openapis.org/oas/v3.0.4.html#operation-object)
and [3.1 Operation Object](https://spec.openapis.org/oas/v3.1.2.html#operation-object).

Admit content-less successful response declarations, including exact, 2XX and
default selectors. Select exact before range before default, following the
[Responses Object](https://spec.openapis.org/oas/v3.1.2.html#responses-object).
Our experimental value correspondence requires verified empty delivered bytes
for a selected content-less success and emits no value. A JSON-declared success
requires valid JSON, including one output for JSON null. Absence of a matching
response declaration retains the earlier JSON-success behavior; this is not
whole-response conformance validation. HTTP 204 and 205 require empty delivery
regardless of source metadata ([HTTP semantics](https://www.rfc-editor.org/rfc/rfc9110.html#name-205-reset-content)).
Incomplete delivery, omitted headers, cancellation or transport errors do not
become empty success. Unexpected bytes are never silently discarded as success.

An alternative is accepting any zero-byte 2xx as no output. Reject that because
it conceals a missing/truncated JSON representation. Another is retaining a
cloned request through execution to select the response declaration; prefer
bounded nonsecret response policy compiled during source analysis, avoiding
retaining copied credentials or a second request payload owner for this purpose.

## Acceptance and exit

- Independent hand-authored OpenBindings documents and literal exchanges for
  list/get/create/replace/patch/delete on 3.0.4 and 3.1.2, frozen before runtime edits.
- Inspection and synthesis expose those targets; synthesized documents preserve
  source and disclose existing schema loss. Cold reload and renamed manual
  operation keys behave consistently through binding and operation invocation.
- Exact JSON and no-output remain distinct; required/absent/null bodies, response
  selector precedence, referenced responses and unsupported neighboring targets
  have explicit positive and negative cases. Verify requests and effects.
- A real native HTTP path covers the new methods. Extracted Cargo consumers and
  a local async Node/Wasm probe exercise the same public APIs. Wasm is not a
  released TypeScript API or browser-network qualification.
- Existing affected suites, format, lint and isolation checks pass. Resource
  limits and cancellation remain effective. No new timing investigation; AJ20
  remains unresolved with unchanged budgets.
- Independent review closes material findings. Record portability observations
  separately from implementation policies. No generic role or published interface
  change without a demonstrated contract defect.

Stop after this capability and the bounded role-contract evidence review. Rich
failure outcomes and synthesis fidelity need their own next assignments. Record
any blocker rather than widening this slice into a full client rewrite.
