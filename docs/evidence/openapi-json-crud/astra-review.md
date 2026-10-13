# Independent Astra reviews

Coordinator record of GPT-6 Astra xhigh read-only reviews on 2026-10-13.

## Role foundations

A separate reviewer inspected selected provider contracts, consumer paths,
checkpoint evidence and non-OpenAPI catalog tests at baseline `d975cc2`. It found
no material blocker to closing the current Rust role foundations. Its scope and
the evidence are recorded in [the role closeout](../../role-contract-closeout.md).
It did not rerun tests or claim API stabilization, publication or TypeScript roles.

## CRUD runtime and consumers

The adapter reviewer accepted response selector precedence, the bounded nonsecret
response policy, empty/null distinction, edition-specific body interpretation and
pre/post-dispatch outcomes. It independently ran the initial four socket-free CRUD
tests successfully and verified all five manually frozen fixture hashes.

The reviewer requested stronger DELETE wire-byte evidence. The shared test host
now asserts the exact supplied body bytes or absence on every dispatch, including
the paired 3.0/3.1 DELETE cases. A fifth test covers a supported PUT beside an
unsupported HEAD and bounded analysis refusal. Final read-only verification
accepted those changes and the README/portability record. A changelog ambiguity
was corrected: the previously accepted invalid payload is specifically HTTP 205;
other newly admitted content-less response operations had been refused before.

Accepted runtime Git blob identities:

- `analysis.rs`: `238da99934b7c2ceab3c84fc3d68fff8803dcc98`
- `invocation.rs`: `f73b3018c752592f7636a3fb7d15fec92df9cbee`

## Qualification tooling and lifecycle

The reviewer verified that native/browser CI now maintains the external consumer.
Product/library/tests and ordinary examples retain strict Clippy. Two style
allowances apply only to the unchanged, stopped `auth_resolution` example; no
benchmark execution was added.

The first new Wasm lifecycle assertion compared post-use owners with a completely
cold process. The corrected probe records cold counts and checks a second suite
against warmed counts after each async export completes. Observed counts were
`0,0 -> 8,0 -> 8,0` (SDK, client). The reviewer confirmed that core's
`embedded_resources()` retains exactly eight schema documents in a process-static
OnceLock. This supports stable ownership across the repeated suite, not zero
process retention or a general memory proof. Existing 1,000-cycle assertions are
unchanged. Final lifecycle review found no material concern.

The later test/tooling/lifecycle checks were read-only. Fresh package/native/Wasm
results belong to the qualification receipt, not these independent review claims.
