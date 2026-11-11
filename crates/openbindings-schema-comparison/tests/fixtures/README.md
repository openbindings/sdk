# Pinned schema-comparison test inputs

Interface-derived inputs target revision
`67f707c31aecbdf0551385e369d8024097db6278`. The crate's tests and guide state the
supported interface/profile scope. Additional SDK controls are identified below;
they are test evidence, not replacements for the pinned publication.

| Fixture | Evidence owner | SHA-256 |
| --- | --- | --- |
| `exact-values.json` | Pinned interfaces | `90b3271bf73af691ec0ae8817308b07e1b1c59868809333350c420b4b84170bf` |
| `independent.json` | SDK controls | `297c0bf62453449b9ad6b5910d49d9f4b540e7ad09ce8c6e04dfe343495cd3f9` |
| `manifest.json` | Pinned interfaces | `069838b89c6e9b63d68e6d0a6d25e5dc502dfd0fca4373cf5a27bc052518931e` |
| `structural/boolean-true-empty-identical.json` | Pinned interfaces | `1cb19206e7269ee4df28dd4346c590f42149901e4b7924ab685706e44260836f` |
| `structural/identical-detects-difference-incompatible.json` | Pinned interfaces | `bf020c92210d71cd15304150e6273a29c0ede283dd81d1a9a7d471728c15d70f` |
| `structural/identity-pattern-difference-incompatible.json` | Pinned interfaces | `326f544565328f71c81202f53534099137536c14c8e5ab558cb3cfaecbeba9ff` |
| `structural/identity-pattern-identical.json` | Pinned interfaces | `e1f8412c3dcec757a34016da1e54ba90f8832a5f6438c273ae72a71e9a74ba06` |
| `structural/normalize-allof-flatten-identical.json` | Pinned interfaces | `336699bfaca86587ba17ba159fbd1e0b928f31d65805c66c132f56a1e147b0f3` |
| `structural/normalize-allof-nested-identical.json` | Pinned interfaces | `5955eafb0904de2d9691291e865660436127f182a9891fa96b17e14080379f98` |
| `structural/normalize-allof-ref-property-merge-identical.json` | Pinned interfaces | `a81664d22300a8198267b11e91f73d7d9a90ac6ef0707728f1c540d8f7f8c9e8` |
| `structural/normalize-allof-siblings-identical.json` | Pinned interfaces | `cd184928bd9a68b7dd2ab8cd4da06fe47d6ebcc25f4dba79bf4171007dd12539` |
| `structural/normalize-annotations-stripped-identical.json` | Pinned interfaces | `d2b080927f051ec45744f9b2d7d8c341dde96af1e530cb94659a2d5ea455fc23` |
| `structural/normalize-ref-inline-identical.json` | Pinned interfaces | `1aa7483dfbeded626be3aae98d8d5abc6ed7be5a3defe0c392b5258bcbe359ac` |
| `structural/normalize-required-canonical-identical.json` | Pinned interfaces | `c415913c8c91bf15c1f100d416d3bdb3c5d79aa020fce0973098b3eac5903766` |
| `structural/normalize-type-canonical-identical.json` | Pinned interfaces | `e6d5fe3a3d748be0de144dab06bcbf7765efa05267512cc5de05217f745825e0` |
| `structural/normalize-union-ordering-identical.json` | Pinned interfaces | `3e2b013096aa00130aea8243937561395ad7cd15e48dd69b2044d1dba0313062` |
| `structural/identical-absent-slot-difference-incompatible.json` | Pinned interfaces | `c65413f99e47d8783179902fbcfaa2870f47550a515edbcf5593bd004e04143e` |
| `structural/identical-absent-slots-identical.json` | Pinned interfaces | `15d268f2afb264ed79bdd5ce35e9640f502a5ed549b5bdf035b49eabeaea2a81` |
| `structural/normalize-allof-closed-branch-retained-identical.json` | Pinned interfaces | `2fe5cf55c3d2d50def9acf1f12403191edc06197913459f4dd74351a6f0ddb5d` |
| `structural/normalize-allof-closed-branch-retained-incompatible.json` | Pinned interfaces | `db9ec74932d4721ad6ae20fd53e9ec1ab6d1e3f7a677fb98ad73ce553186b163` |
| `structural/normalize-allof-union-retained-identical.json` | Pinned interfaces | `dc65a93f86e8cfe6ff9cd0368d4df118a598bdcf1cb096d8b2b66acc60b2abe0` |
| `structural/normalize-allof-union-retained-incompatible.json` | Pinned interfaces | `aeb2fef76c991b9c2d28e5f352d1110f4a86a13871124907b6ca20187f6f2ef7` |
| `structural/normalize-defs-kept-for-retained-reference-identical.json` | Pinned interfaces | `f9e4df4d89b91f354e0096940f898194e2fae9b72c60db04f0ef3b05e5b2e0b9` |
| `structural/normalize-defs-kept-for-retained-reference-incompatible.json` | Pinned interfaces | `6be0e5469ae1a63e19b3dd55bf33a5a0fde4218edc394c45dc3e86ec2b23a036` |
| `structural/normalize-ref-resource-difference-incompatible.json` | Pinned interfaces | `96e832d75efc5e6bc9f184dad29794b23e3ec8279fd2ce8836bdd7ec9c7afdf9` |
| `structural/normalize-ref-resource-identical.json` | Pinned interfaces | `164634563ce8d0bf0271d4149664fc71cb11e2d3cdb75e72c34cf723e3c486de` |
| `structural/normalize-ref-siblings-difference-incompatible.json` | Pinned interfaces | `7f2ad2cf13e7d80e965ca27c1ca8cbfecbba285915057c15aae9e7a8d4d36b3f` |
| `structural/normalize-ref-siblings-identical.json` | Pinned interfaces | `22f34a989475acda60b31de2b10b2a0fa8c41249a86a547901c86bd5a2f9fc06` |
| `structural/nullable-outside-profile-difference-incompatible.json` | Pinned interfaces | `5d43fe3ae410270a641550ab2dabe8ef2c7aadcc9407145403e0c30b0275bb15` |
| `structural/nullable-outside-profile-identical.json` | Pinned interfaces | `db8a28e6ab315cd60bd0ac9d91ab247fc9951c4933304327335b628d88b10695` |
