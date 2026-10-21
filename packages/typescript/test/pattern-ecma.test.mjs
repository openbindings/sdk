import test from "node:test";
import assert from "node:assert/strict";
import {
  checkedIn,
  fixture,
} from "../../../crates/openbindings-json-schema-evaluator/tests/fixtures/pattern-ecma.mjs";

// The evaluator's pattern reading is compared with this fixture's results.
// Generating the rows again and deriving their results from this engine's
// RegExp on every run keeps that comparison an ECMAScript differential rather
// than a comparison with a past run.
test("the generated patterns' recorded results are ECMAScript RegExp results", () => {
  const { node: _node, v8: _v8, ...recorded } = checkedIn();
  const { node: _thisNode, v8: _thisV8, ...derived } = fixture();
  assert.deepEqual(derived, recorded);
});
