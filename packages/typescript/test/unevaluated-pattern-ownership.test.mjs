import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { prepareInput } from "./conformant-contracts.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
// The native test reads the same cases; key order in each schema is kept, as
// it decides what the library compiles first.
const cases = JSON.parse(
  await readFile(
    new URL(
      "../../../crates/openbindings-json-schema-evaluator/tests/fixtures/unevaluated-pattern-ownership.json",
      import.meta.url,
    ),
    "utf8",
  ),
);
const outcome = { satisfies: "S", fails: "F" };
test("patternProperties subschemas reached through references stay evaluable through Wasm", () => {
  assert.equal(cases.length, 4);
  for (const c of cases) {
    const parsed = sdk.Document.parse(JSON.stringify(c.document));
    assert.equal(parsed.status, "parsed", c.id);
    const values = c.resources.map((r) => [
      r.uri,
      sdk.parseJson(JSON.stringify(r.schema)).value,
    ]);
    const resources = new sdk.SchemaResources(values);
    let contract;
    try {
      const setup = prepareInput(parsed.value, "op", { resources });
      assert.equal(setup.status, "ready", c.id);
      contract = setup.valueContract;
      c.values.forEach((value, index) => {
        const label = `${c.id} ${JSON.stringify(value)}`;
        const expected = c.expected[index];
        assert.equal(
          outcome[contract.validate(value).outcome],
          expected,
          label,
        );
        assert.equal(outcome[contract.explain(value).outcome], expected, label);
      });
    } finally {
      contract?.dispose();
      resources.dispose();
      for (const [, value] of values) value.dispose();
      parsed.value.dispose();
    }
  }
});
