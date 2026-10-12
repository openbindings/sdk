# Interface snapshots and SDK policy evidence

Interface source revision: `f158f4f345301668ccb8fd555586b04ca823e732` on
`main` (interfaces PR #45). These source snapshots do not establish a package
release or website deployment.

| Fixture | Evidence owner | SHA-256 |
| --- | --- | --- |
| `binding-invoker-0.1.json` | Pinned interface source | `9f611c87148613b2f50f5694a2c7135aa3c255faa536ed8e3faf720ff573355c` |
| `operation-invoker-0.1.json` | Pinned interface source | `164988547585a7faf983ccf569ebe742b39dcd4082d9b80252230f690fc4f7d2` |

The binding and operation JSON files are verbatim interface snapshots.
`frame_correspondence.rs` checks the declared frame shapes and separately exercises
the SDK test bridge's error-handling and lifecycle policies. Native queue draining,
terminal-race arbitration, polling and malformed-frame classification are SDK
decisions, not obligations of every interface adopter.
