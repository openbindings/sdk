# Original evaluator: prepared reference queries

Preparation already resolves activated original references to discover the graph.
It now retains those typed results in the immutable Plan, keyed by original node
and reference keyword within one original environment. Evaluation shares the
result without reparsing URI text or copying result strings. Dynamic and recursive
scope selection remains per evaluation. Failed queries retain their typed meaning;
an irrelevant missing target does not become a blanket preparation refusal.

Map/Arc construction and extra retained URI copies are reserved before allocation.
No request or preparation control is retained. New controls cover identical relative
spellings at different bases, missing-content versus independent branches, retained
owners, cancellation/retry, and repeated evaluation of a 10,000-character URI under
128 evaluation work units. The latter removes work rather than lowering its cost.

Native and Rust 1.88 evaluator suites, required-value replays (1,546 each), strict
current lint, Wasm/Node (180), type/reference checks and Chromium/WebKit independent
judgments (505 core plus 1,546 evaluator values each) pass. Independent review passes at 511a1bf, including alternating dynamic/recursive
scopes, original bases and snapshots, malformed/missing/decisive outcomes and all
194 observed cancellation boundaries with clean retry. This does not close repeated syntax-accounting,
diagnostic, retained-memory or whole-engine performance gates.
