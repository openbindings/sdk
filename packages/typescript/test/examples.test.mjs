import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { initialize, liveStorageOwners } from "../dist/index.js";
import { exampleDocument, firstUse } from "../examples/first-use.mjs";

await initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);

test("first-use caller keeps setup refusals separate and releases its owners", () => {
  firstUse(); // warm complete preparation before recording arena retention
  const baseline = liveStorageOwners();
  const accepted = firstUse({ id: 7 });
  assert.equal(accepted.status, "checked");
  assert.equal(accepted.result.outcome, "satisfies");
  assert.equal(accepted.operations[0].key, "lookup");
  assert.deepEqual(accepted.operations[0].aliases, ["find"]);
  assert.equal(
    firstUse({ id: 7 }, exampleDocument, "lookup").result.outcome,
    "satisfies",
  );
  assert.equal(firstUse({ id: 0 }).result.outcome, "mismatch");
  const admission = firstUse({ id: NaN }).result;
  assert.equal(admission.outcome, "input-error");
  assert.equal(admission.error.instancePointer, "/id");
  assert.equal(
    firstUse({}, exampleDocument, "missing").status,
    "operation-missing",
  );
  assert.equal(firstUse({}, "{").status, "input-error");
  const invalid = firstUse(
    {},
    '{"openbindings":"0.2.0","operations":{"lookup":{"description":42}}}',
  );
  assert.equal(invalid.status, "assessed");
  assert.equal(invalid.report.conclusion, "non-conformant");
  assert.equal(liveStorageOwners(), baseline);
});
