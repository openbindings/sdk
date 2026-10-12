# Interface snapshots and SDK policy evidence

Interface source revision: `468bb83e5986de3ee490767bcb4b6630071bdec5` on
`codex/interface-boundary-repair`. These are draft source snapshots, not evidence
of a deployed or released publication.

| Fixture | Evidence owner | SHA-256 |
| --- | --- | --- |
| `binding-invoker-0.1.json` | Pinned interface source | `6f3f1bf1438a2b51b6fdb9fafa04b58d7cb8d979bdda38aecbfb9a4935940d21` |
| `operation-invoker-0.1.json` | Pinned interface source | `184e46e0d8041af7429c4a41cdf5cb729fbebdd36b5a74d588e17ec600902efb` |

The binding and operation JSON files are verbatim interface snapshots.
`frame_correspondence.rs` checks the declared frame shapes and separately exercises
the SDK test bridge's error-handling and lifecycle policies. Native queue draining,
terminal-race arbitration, polling and malformed-frame classification are SDK
decisions, not obligations of every interface adopter.
