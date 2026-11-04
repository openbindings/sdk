# Pinned schema-comparison test inputs

Interface-derived inputs target revision
`5815fb106bddd2c85ff4e93e0077570b3f6b9e74`. The crate's tests and guide state the
supported interface/profile scope. Additional SDK controls are identified below;
they are test evidence, not replacements for the pinned publication.

| Fixture | Evidence owner | SHA-256 |
| --- | --- | --- |
| `exact-values.json` | Pinned interfaces | `90b3271bf73af691ec0ae8817308b07e1b1c59868809333350c420b4b84170bf` |
| `independent.json` | SDK controls | `8a899c9d2f2fdb89dca0532ce85f3926c0397f9db21f8e0101531ef7d748a119` |
| `manifest.json` | Pinned interfaces | `79eac6f7b2b59533867aeeb30f1c95f8510ba2d65b1628140f0c9a4daf0c3829` |
| `structural/boolean-true-empty-identical.json` | Pinned interfaces | `1cb19206e7269ee4df28dd4346c590f42149901e4b7924ab685706e44260836f` |
| `structural/identical-detects-difference-incompatible.json` | Pinned interfaces | `bf020c92210d71cd15304150e6273a29c0ede283dd81d1a9a7d471728c15d70f` |
| `structural/identity-pattern-difference-incompatible.json` | Pinned interfaces | `326f544565328f71c81202f53534099137536c14c8e5ab558cb3cfaecbeba9ff` |
| `structural/identity-pattern-identical.json` | Pinned interfaces | `e1f8412c3dcec757a34016da1e54ba90f8832a5f6438c273ae72a71e9a74ba06` |
| `structural/normalize-allof-flatten-identical.json` | Pinned interfaces | `336699bfaca86587ba17ba159fbd1e0b928f31d65805c66c132f56a1e147b0f3` |
| `structural/normalize-allof-nested-identical.json` | Pinned interfaces | `5955eafb0904de2d9691291e865660436127f182a9891fa96b17e14080379f98` |
| `structural/normalize-allof-ref-property-merge-identical.json` | Pinned interfaces | `a81664d22300a8198267b11e91f73d7d9a90ac6ef0707728f1c540d8f7f8c9e8` |
| `structural/normalize-allof-siblings-identical.json` | Pinned interfaces | `cd184928bd9a68b7dd2ab8cd4da06fe47d6ebcc25f4dba79bf4171007dd12539` |
| `structural/normalize-annotations-stripped-identical.json` | Pinned interfaces | `d2b080927f051ec45744f9b2d7d8c341dde96af1e530cb94659a2d5ea455fc23` |
| `structural/normalize-nullable-identical.json` | Pinned interfaces | `df8b237415d54a7ef9ddc26bef4f1093737c053913511946c82a5a9a7e7d99ce` |
| `structural/normalize-ref-inline-identical.json` | Pinned interfaces | `1aa7483dfbeded626be3aae98d8d5abc6ed7be5a3defe0c392b5258bcbe359ac` |
| `structural/normalize-required-canonical-identical.json` | Pinned interfaces | `c415913c8c91bf15c1f100d416d3bdb3c5d79aa020fce0973098b3eac5903766` |
| `structural/normalize-type-canonical-identical.json` | Pinned interfaces | `e6d5fe3a3d748be0de144dab06bcbf7765efa05267512cc5de05217f745825e0` |
| `structural/normalize-union-ordering-identical.json` | Pinned interfaces | `3e2b013096aa00130aea8243937561395ad7cd15e48dd69b2044d1dba0313062` |
