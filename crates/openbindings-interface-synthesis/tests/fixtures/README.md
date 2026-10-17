# Interface snapshots and SDK policy evidence

Interface source revision: `f158f4f345301668ccb8fd555586b04ca823e732` on
`main` (interfaces PR #45). These source snapshots do not establish a package
release or website deployment.

| Fixture | Evidence owner | SHA-256 |
| --- | --- | --- |
| `coverage-cases.json` | SDK policy regression | `ef59a08af9907cbc90c13effe9a403238318ecad6c529e4f4492251e1a6404e2` |
| `coverage-fixture.schema.json` | SDK policy regression | `57763fc6d43ece95e52cbd717190c6b78b58bd4fcbb941a040118e478708f5d8` |
| `interface-output-cases.json` | Pinned interface source | `c957f42f3aff41fd85c88360656f6a8a3d2482c128bee4a8b63c9f9ae79dd7c7` |
| `interface-synthesizer-0.2.json` | Pinned interface source | `379da04078b19770d9ce2b60a1b3c747bbf20645ba739460abfc2181b52929ee` |
| `conformance/kind-support/cases.json` | Pinned interface source | `109d33ceb52bb77264e0856d37506a46cfa474516d0ffbeff8496b3041609740` |

`conformance/kind-support/cases.json` is a verbatim copy of the same path in that
revision's published corpus; `conformance_corpus.rs` runs every case through the
synthesis facade.

`interface-synthesizer-0.2.json` and `interface-output-cases.json` are verbatim
interface snapshots. The latter comes from `conformance/synthesis-coverage/cases.json`.
The 28 `coverage-cases.json` cases are SDK-owned regressions, adapted
from interfaces `56aadc07aef362d1bbe1f42dd3081f4a750555f6` and identified by
`openbindings.sdk-synthesis-policy-cases@1`. They no longer claim portable rejection,
source-preservation or strict-eligibility requirements. The published interface permits unbound
operations, optional dependency attribution, and many-to-one evidence. Supplied
dependency keys must still exist in the emitted map.
The interface shape examples independently allow optional attribution and other
variation; no schema test proves source truth or kind soundness.
