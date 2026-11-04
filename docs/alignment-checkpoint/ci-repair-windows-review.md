# Windows fixture-byte preservation follow-up

**PASS for the bounded `.gitattributes` correction against 6988e6d.** The two deleted companion paths are replaced with the four current context, kind-description, schema-comparison and source-pull fixture subtrees. Existing internal-schema, invocation, evaluator and attribution rules are unchanged. `-text` disables line-ending conversion; neither recorded digests nor validation/expected bytes are normalized.

The independent scratch-repository probe in `check-attributes.py` applies Git's actual checkout filters with `core.autocrlf=true` to all **28** files in the four added fixture subtrees. At the committed pre-fix attributes **0/28** preserve the original bytes; at the reviewed working attributes **28/28** preserve them. `attributes-observations.json` retains per-file original/smudged hashes. This directly reproduces and closes the checkout cause; a hosted Windows run remains an integration gate rather than a result inferred from this simulation.

The scope includes fixture README files, whose recorded hashes and publication provenance remain intact. This change does not expand support claims or weaken exact-byte checks. The previous 22-file CI review remains separate evidence; this follow-up adds one reviewed source file.
