# Pinned synthesis role and portable cases

Verbatim copies from `openbindings/interfaces` revision
`9e8fcff80643ff4ef30415dc782489db65789ef9` on `codex/interface-synthesis-contract`.
The role is Interface Synthesizer 0.2.0, independent of SDK alpha package versions.

| SDK fixture | Interfaces source | SHA-256 |
| --- | --- | --- |
| `interface-synthesizer-0.2.json` | `interface-synthesizer/0.2.json` | `54456dcf621d8a12e04b2e4a36ab5e1ab3af1bbb3c7bbbf3001b23b9cddf1d0c` |
| `coverage-cases.json` | `conformance/synthesis-coverage/cases.json` | `01f9bc37e4ddbc947af59331c4e0bdb5582636ad1d20ec66a50b1acddf59d3aa` |
| `coverage-fixture.schema.json` | `conformance/synthesis-coverage/fixture.schema.json` | `9a65a2b07a79a8d7242a253163a60cc772c42ecdad0bb5fdbd73f84ae986ea93` |

The output schema is an independent wire-shape oracle. The 17 portable cases
also check prose requirements including dependency correspondence and derived
completeness. Native boundary and external consumer tests cover request context,
provider ownership and remaining C1–C4 obligations. No schema alone proves truthful
source enumeration or kind soundness.
