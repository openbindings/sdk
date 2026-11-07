// Recheck the frozen generated syntax controls with their independent engine.
// This constructs regular expressions but never executes them against a value.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const fixture = JSON.parse(
  readFileSync(
    new URL(
      "../crates/openbindings-json-schema-evaluator/tests/fixtures/regex-format-node-controls.json",
      import.meta.url,
    ),
  ),
);
assert.equal(
  process.version,
  fixture.oracle,
  "use the recorded independent oracle version",
);
let valid = 0;
for (const row of fixture.cases) {
  const source = JSON.parse(row.source_json);
  let accepted = true;
  try {
    new RegExp(source);
  } catch (error) {
    assert(error instanceof SyntaxError);
    accepted = false;
  }
  assert.equal(
    accepted,
    row.valid,
    `syntax oracle changed for ${row.source_json}`,
  );
  valid += Number(accepted);
}
console.log(
  JSON.stringify({
    oracle: process.version,
    cases: fixture.cases.length,
    valid,
    failed: 0,
  }),
);
