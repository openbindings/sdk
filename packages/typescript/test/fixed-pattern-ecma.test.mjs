import test from "node:test";
import assert from "node:assert/strict";
import {
  checkedIn,
  fixture,
} from "../../../crates/openbindings-internal-schema/tests/fixtures/fixed-pattern-ecma.mjs";

// The native fixed checks are compared with this fixture's results. Deriving
// them again from this engine's RegExp on every run keeps that comparison an
// ECMAScript differential rather than a comparison with a past run.
test("the fixed patterns' recorded results are ECMAScript RegExp results", () => {
  const { node: _node, v8: _v8, ...recorded } = checkedIn();
  const { node: _thisNode, v8: _thisV8, ...derived } = fixture();
  assert.deepEqual(derived, recorded);
});
