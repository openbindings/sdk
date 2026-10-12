# Interface snapshots and SDK policy evidence

Interface source revision: `468bb83e5986de3ee490767bcb4b6630071bdec5` on
`codex/interface-boundary-repair`. These are draft source snapshots, not evidence
of a deployed or released publication.

| Fixture | Evidence owner | SHA-256 |
| --- | --- | --- |
| `coverage-cases.json` | SDK policy regression | `362b6b5fdc1643c09c22c27dea34f9b2536fdd9e422f82d03431bdbbb2b54703` |
| `coverage-fixture.schema.json` | SDK policy regression | `57763fc6d43ece95e52cbd717190c6b78b58bd4fcbb941a040118e478708f5d8` |
| `interface-output-cases.json` | Pinned interface source | `c957f42f3aff41fd85c88360656f6a8a3d2482c128bee4a8b63c9f9ae79dd7c7` |
| `interface-synthesizer-0.2.json` | Pinned interface source | `bcc74d800d2216863bb280f13d142442015c1930970b6a6bee1c60983c9af16d` |

`interface-synthesizer-0.2.json` and `interface-output-cases.json` are verbatim
interface snapshots. The latter comes from `conformance/synthesis-coverage/cases.json`.
The 26 `coverage-cases.json` cases are now SDK-owned policy regressions, preserved
from interfaces `56aadc07aef362d1bbe1f42dd3081f4a750555f6` and identified by
`openbindings.sdk-synthesis-policy-cases@1`. They no longer claim portable rejection,
source-preservation, strict-eligibility or dependency-bijection requirements.
The interface shape examples independently allow optional attribution and other
variation; no schema test proves source truth or kind soundness.
