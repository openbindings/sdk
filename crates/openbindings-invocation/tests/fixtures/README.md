# Pinned current invocation fixtures

These byte-exact sources target interfaces revision
`a3c19658fc89815d7a417eae29ba31fd4e930daf` and core specification revision
`c1419c9fc3ae5a00c2b83da710c11bf67dab7934`.

| Fixture | SHA-256 |
| --- | --- |
| `binding-invoker-0.1.json` | `cf615d4d4677aa9d1225accd02ac50ab0bc4b8644aac5fcc8c74f99d1c646ec8` |
| `operation-invoker-0.1.json` | `5e8945e4ef96aacdc32594acc0b6635f5ed4242c38e31b4333953acfe2b6a02a` |
| `conformance/selection/automatic-resolution.json` | `deb5a6c6ff58e12bed35441993816dadcdf5d451f99ec721f097ce60d8c59a18` |
| `conformance/selection/default-supported.json` | `f13429bb6ade26e14fab664bae0020e399f31eac0cd6b8edc16f9be273f5d791` |
| `conformance/selection/explicit-binding.json` | `ba2e39597dfa15448052a195835f79cb5028b4c4c4070b2739e25e17b5b577bb` |
| `conformance/selection/override-selection.json` | `12d6e91b627e238516a57b52ec5b59e543babd9b250feb1e6ade23a76e8ad2e0` |

The twenty published selection cases run through plain invocation, prepared
invocation, and preflight. The archived frame-correspondence experiment consumed the current
interface snapshots; its adapter and native lifecycle policies are SDK choices.
The previous kind-support query is absent from the current invocation contract.
Kind description is a separate companion obligation, not a passing invocation
case. Original previous-revision fixtures and test adjudication are retained in
the migration receipts; no old interface compatibility is promised.
