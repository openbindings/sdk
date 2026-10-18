# Interface snapshots and SDK policy evidence

Interface source revision: `f158f4f345301668ccb8fd555586b04ca823e732` on
`main` (interfaces PR #45). These source snapshots do not establish a package
release or website deployment.

| Fixture | Evidence owner | SHA-256 |
| --- | --- | --- |
| `source-inspector-0.1.json` | Pinned interface source | `dcaca9bccb4312af92d6907988b9253c5392e8bcd093f8f3991e9a320faf133b` |
| `conformance/kind-support/cases.json` | Pinned interface source | `109d33ceb52bb77264e0856d37506a46cfa474516d0ffbeff8496b3041609740` |

`conformance/kind-support/cases.json` is a verbatim copy of the same path in that
revision's published corpus; `conformance_corpus.rs` runs every case through the
inspection facade.

The output-schema tests check the described result shape. The SDK separately
rejects duplicates and malformed carriers. That policy is not an interface
requirement, and shape acceptance is not evidence of truthful inspection.
