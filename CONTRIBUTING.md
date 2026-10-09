# Contributing

Work against `main` in a short-lived branch and submit a pull request. Source
changes use squash merge after the required SDK qualification checks pass.
Repository identity is independent of implementation language; today Rust owns
the semantic engine and TypeScript exposes a first-class API through Wasm.

Follow the documented core/optional-package boundaries in docs/architecture.md.
Run the source replay in docs/REPLAY.md. The CI matrix verifies native targets and
real browser hosts; its definitions do not imply unexecuted jobs have passed.
Keep tests and diagnostics focused on documented semantics and real callers.

Package publication, version selection and consumer migration follow RELEASING.md
and require their own release decision. Do not publish from this source-import
workflow. Historical benchmarks retain their workload and host limits.
