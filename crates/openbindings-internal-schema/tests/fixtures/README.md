# Fixed-pattern ECMAScript oracle

The fixed checks' five patterns must match exactly as ECMAScript's `u`-mode
RegExp does. `fixed-pattern-ecma.mjs` generates a 60,000-string corpus from a
seed (versions, names, identifiers and random pieces, with line terminators,
astral characters, unpaired surrogates and the fixed-string presentation's
private-use marker) and records, for each pattern, V8's
`new RegExp(pattern, "u").test(string)` result per string, with the Node and V8
versions that computed them. `../support/ecma_patterns.rs` regenerates the same
corpus, checks its digest and compares the shipped engine through every keyword
that can carry a pattern; the TypeScript suite's `fixed-pattern-ecma.test.mjs`
derives the recorded results from V8 again on every run. Regenerate the fixture
with `node crates/openbindings-internal-schema/tests/fixtures/fixed-pattern-ecma.mjs --write`.

| Fixture | Evidence owner | SHA-256 |
| --- | --- | --- |
| `fixed-pattern-ecma.json` | V8 12.4.254.21-node.29 (Node v22.19.0) | `2dc2665e410e03d6a999209961b37483c5cf46b41404aae6bb170bb96418112d` |
| `fixed-pattern-ecma.mjs` | SDK generator | `8a4a68548501b26f84db31fb296cdd50006b3165c1ee91a601e5858e62768225` |
