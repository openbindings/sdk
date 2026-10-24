import test from "node:test";
import assert from "node:assert/strict";
import {
  checkedIn,
  fixture,
  laterEditionSyntax,
} from "../../../crates/openbindings-json-schema-evaluator/tests/fixtures/pattern-ecma.mjs";

const text = (units) => String.fromCharCode(...units);

// The evaluator reads patterns in the dialect JSON Schema 2020-12 cites,
// ECMA-262 11th edition (ES2020) with Unicode semantics, and is compared with
// the checked-in fixture's results for it. Generating the rows again and
// deriving their results from this engine's RegExp on every run keeps that
// comparison an ECMAScript differential rather than a comparison with a past
// run, wherever this engine reads the 11th edition's syntax. Where a pattern
// uses syntax a later edition added, a newer engine accepts what the 11th
// edition refuses, so the fixture's refusal is the oracle there.
test("the generated patterns' recorded results are ECMA-262 11th edition RegExp results", () => {
  const { node: _node, v8: _v8, rows: recorded, ...spec } = checkedIn();
  const {
    node: _thisNode,
    v8: _thisV8,
    rows: derived,
    ...thisSpec
  } = fixture();
  assert.deepEqual(thisSpec, spec);
  assert.equal(derived.length, recorded.length);
  const later = new Map();
  for (const [index, row] of derived.entries()) {
    const expected = recorded[index];
    const pattern = text(row.pattern);
    assert.deepEqual(
      { pattern: row.pattern, strings: row.strings },
      { pattern: expected.pattern, strings: expected.strings },
      `row ${index} is generated again`,
    );
    const classes = laterEditionSyntax(pattern);
    if (classes.length === 0) {
      assert.equal(row.matches, expected.matches, JSON.stringify(pattern));
      continue;
    }
    assert.equal(
      expected.matches,
      null,
      `the 11th edition refuses ${classes.join(" and ")}: ${JSON.stringify(pattern)}`,
    );
    for (const name of classes) later.set(name, (later.get(name) ?? 0) + 1);
  }
  // Both classes occur, so the exception is exercised wherever it applies.
  assert.deepEqual([...later.keys()].sort(), [
    "duplicate named groups",
    "modifiers",
  ]);
});
