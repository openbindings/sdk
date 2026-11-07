# Windows evaluator-corpus byte preservation

**PASS for the one-rule `.gitattributes` follow-up against e59316f.** The added `/crates/openbindings-schema-evaluator-test-support/fixtures/** -text` rule protects the adjudicated corpus's actual directory. It does not replace the distinct evaluator implementation's `tests/fixtures` rule or change any earlier fixture/attribution rule. No production files were edited by the reviewer.

The independent `check-evaluator-attributes.py` probe uses Git's actual checkout filters in a scratch repository with `core.autocrlf=true`. Before the rule, **0/11** files preserve their bytes and the unchanged input verifier fails with `input hash drift: cases.json`. After the rule, **11/11** files are byte-identical and the verifier run on those checkout-filtered files passes: **1,566 historical cases, 1,546 required value cases, 20 negative admission cases, nine rejected mutations**. The corpus includes its manifest, provenance, retained historical inputs, current decisions, and license files.

`evaluator-attributes-observations.json` records all original/checkout hashes, the reviewed attributes hash, the unchanged verifier hash, and both gate results. This is checkout preservation, with no normalization of expected/input bytes, oracle change, or weakened comparison. A hosted Windows run remains a separate integration result.
