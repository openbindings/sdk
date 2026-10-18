# Interface snapshots and SDK policy evidence

Interface source revision: `f158f4f345301668ccb8fd555586b04ca823e732` on
`main` (interfaces PR #45). These source snapshots do not establish a package
release or website deployment.

| Fixture | Evidence owner | SHA-256 |
| --- | --- | --- |
| `binding-invoker-0.1.json` | Pinned interface source | `9f611c87148613b2f50f5694a2c7135aa3c255faa536ed8e3faf720ff573355c` |
| `operation-invoker-0.1.json` | Pinned interface source | `164988547585a7faf983ccf569ebe742b39dcd4082d9b80252230f690fc4f7d2` |
| `conformance/kind-support/cases.json` | Pinned interface source | `109d33ceb52bb77264e0856d37506a46cfa474516d0ffbeff8496b3041609740` |
| `conformance/selection/automatic-resolution.json` | Pinned interface source | `b36ddea2f985f106c758174f6ba66b770dce01b35edf85808c01ba87f89104b4` |
| `conformance/selection/default-supported.json` | Pinned interface source | `f2d58689bf9d87b604a9ccfc55e0ff13dce070e91637c18eeccac3af63b97f54` |
| `conformance/selection/explicit-binding.json` | Pinned interface source | `cee6ca3396b0d9983dbbe0486b5bb0c7dae4f60fc215939acc0f668e9d44021f` |
| `conformance/selection/override-selection.json` | Pinned interface source | `4f142c38277066cfe687ac43350cfce12d7b2ca4608f902cf0929f7bfd9dc263` |

The binding and operation JSON files are verbatim interface snapshots. The
`conformance/` files are verbatim copies of the same paths in that revision's
published corpus. `conformance_corpus.rs` runs every selection case through plain
invocation, a prepared operation and preflight, and every kind-support case
through the binding facade.
`frame_correspondence.rs` checks the declared frame shapes and separately exercises
the SDK test bridge's error-handling and lifecycle policies. Native queue draining,
terminal-race arbitration, polling and malformed-frame classification are SDK
decisions, not obligations of every interface adopter.
