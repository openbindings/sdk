# Rust role contract closeout

The optional Rust inspection, synthesis, binding invocation and operation invocation
foundations are complete within their current documented scope. The bounded review
of SDK `d975cc2` identified no material contract or consumer gap requiring a redesign
before extending the OpenAPI adapter. This is a foundation checkpoint; stabilization,
publication and first-class TypeScript role APIs remain separate work.

The core document model remains independent of these companions. Kind providers own
interpretation and effects. Inspection and synthesis can use shared private analysis
without depending on one another. Operation invocation selects a document binding
and composes the same binding-provider lifecycle, with explicit value evaluation.

## Evidence supporting closure

- The non-OpenAPI catalog consumer exercises inspection, synthesis and invocation
  in [the external consumer](../examples/binding-invocation-consumer/tests/consumer.rs),
  including every supported mode/scope/disposition combination. Synthesis-only and
  invoker-only consumers establish independent use of the roles.
- [Inspection](../examples/source-inspection-consumer/tests/consumer.rs),
  [synthesis](../examples/interface-synthesis-consumer/tests/consumer.rs) and
  [operation consumers](../examples/binding-invocation-consumer/tests/operation.rs)
  exercise borrowed non-Clone providers, shared owners, local Rc hosts and conditional
  Send futures. Exact output and coverage evidence survive disposal of their inputs.
- Synthesis tests preserve partial results and source attribution through strict
  recovery. [Operation cancellation regressions](../crates/openbindings-invocation/tests/operation_cancellation.rs)
  cover rejection without false EOF, cooperative cancellation and terminal precedence;
  the binding lifecycle retains committed output and settles once.
- The [interface boundary repair](interface-boundary-repair.md) governs the distinction
  between reusable published role shapes and SDK checker policies. Maintained tests
  explicitly admit independent interface implementations with different policies.

GPT-6 Astra xhigh independently inspected these paths, selected tests and historical
qualification receipts and found no material blocker. That review was read-only and
did not claim fresh test execution or exhaustive correctness. The adjacent CRUD
qualification reruns the affected role suites; its receipt records its own source.

## Remaining boundaries

Cancellation is cooperative and promises no rollback. Provider truthfulness about
source meaning and effects cannot be established by structural checking alone.
Finite queues and encoding limits do not measure total process memory. Runtime trait
erasure, a production remote frame transport, first-class TypeScript role APIs and
additional kind providers are outside this checkpoint.

Reopen a role contract only for a concrete consumer or provider problem. Broader
OpenAPI support does not itself require another generic role design cycle. Historical
slice reports retain their original provisional status and results; this closeout
does not rewrite those reports or promote SDK policy into an interface requirement.
