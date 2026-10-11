# Pinned synthesis role and portable cases

Verbatim copies from `openbindings/interfaces` revision
`56aadc07aef362d1bbe1f42dd3081f4a750555f6` on `codex/interface-synthesis-contract`.
The role is Interface Synthesizer 0.2.0, independent of SDK alpha package versions.

| SDK fixture | Interfaces source | SHA-256 |
| --- | --- | --- |
| `interface-synthesizer-0.2.json` | `interface-synthesizer/0.2.json` | `b26691a0cae345eb1b22093526d99125f38f47c97c9626abf22371bb150468bc` |
| `coverage-cases.json` | `conformance/synthesis-coverage/cases.json` | `a9fdd0612a5ff261e934b2f8d8cb6b491b0aec5c74296a19e4bba90f1dd397ff` |
| `coverage-fixture.schema.json` | `conformance/synthesis-coverage/fixture.schema.json` | `647f76212ff42907e983fc27d39dc65024e14c8161841c0fce023d6f64d4df68` |

The output schema is an independent wire-shape oracle. The 26 portable cases
also check prose requirements including dependency correspondence and derived
completeness. Native boundary and external consumer tests cover request context,
provider ownership and remaining C1–C6 obligations. No schema alone proves truthful
source enumeration or kind soundness.
