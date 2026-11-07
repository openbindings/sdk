# R-SP2 independent follow-up — borrowed-provider conditional Send

2026-11-03. **PASS for this correction and sibling APIs.** No production edits were made. `R-SP2-final-source-hashes.json` pins the reviewed clients, shared finite-attempt helper and permanent regressions. This supersedes the unresolved R-SP2 note in the initial bounded source-pull review; it does not close the separate ordinary-compiler allocation gate.

## Finding and preserved evidence

**P2, resolved:** the original async client method made a valid non-static provider's caller future fail `Send` checking, despite the selected provider GAT future being Send. A generic caller constrained only at the actual borrowing lifetime incorrectly needed that lifetime to be static / the GAT to be Send for every lifetime. The public provider traits themselves required neither restriction. The external consumer found the missing obligation after the earlier stateless/Cell/Rc checks had passed; the previous PASS did not establish this generic borrowing case.

`R-SP2-before.log` preserves the independent source-pull compile failure. Naming the method lifetime alone (`R-SP2-after.log`) and explicitly selecting the shared helper's future type (`R-SP2-explicit-helper.log`) still failed. `R-SP2-kind-sibling-before.log` independently reproduces the same error for KindClient::list_kinds, KindClient::describe_kind and SourceCheckClient::check_source. The pre-fix kind client is preserved unchanged in `R-SP2-kind-client-before.rs`.

## Correction reviewed

Each public method now selects exactly `P::Pull<'a>`, `P::List<'a>`, `P::Describe<'a>` or `P::Check<'a>` at its non-async boundary. A private async function stores a concrete `F` plus the constructor closure. No associated projection is left as the private async function's future parameter; there is no blanket Send/Sync/static requirement, type erasure, or eager provider call. Copying limits and constructing the closure are the only eager work.

The checked bodies preserve the previous order: request bounds/admission before provider construction, the shared isolated-child lifecycle, exact result identity and carrier limits, relationship/conformance checks for pulls, and cancellation before handoff. The helper itself is unchanged. Constructor panic remains guarded; completion or abandonment cancels the child before provider-future destruction; relay/work cleanup finishes before the final external-token check. Local Rc outcomes/futures remain usable, and a Send future need not be Sync.

## Independent rerun

- Current Rust 1.99: **36 source-pull groups** pass, including the unchanged 74 relationship controls, independent 500-case closure oracle, allocation controls, constructor/poll/destructor cancellation, all-checkpoint validation retry, and the generic non-static Send caller across borrowed/owned/Box/Arc providers. The new sibling compile contracts also pass. Evidence: `R-SP2-complete-pull.log`.
- Current Rust 1.99: **32 kind groups** pass in an isolated copy of current source. These include the original 14 pinned carrier cases, thirteen independent carrier/client/lifecycle groups, the new generic contracts, and all three operations across four non-static provider ownership forms. Evidence: `R-SP2-complete-kind.log`.
- Rust 1.88: **9 focused pull/sibling groups** and the same **32 kind groups** pass. Evidence: `R-SP2-msrv-pull.log`, `R-SP2-msrv-kind.log`. This review did not run a mismatched Clippy driver under MSRV.

The pull witness suspends once, resumes while retaining a reference to stack state, then keeps the returned owned result after disposing of the client/provider. The kind production witness covers twelve operation/ownership combinations; the independent generic contracts require Send only for each selected lifetime. Existing cancellation tests cover cleanup-triggered cancellation overriding successful, operational-error and portable-failure handoffs, pre-cancelled dispatch, future drop isolation, cross-thread synchronous cancellation and latest-waker registration.

No new blocker remains in this bounded API correction. Broader role-consumer and Wasm qualification remains recorded by the root agent, rather than being inferred from these native checks.
