import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import * as testing from "../dist/testing.js";
import { originalExplainCases } from "./original-explain.mjs";
import { prepareInput } from "./conformant-contracts.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const controls = JSON.parse(
  await readFile(
    new URL(
      "../../../crates/openbindings-json-schema-evaluator/src/semantic/explain/controls.json",
      import.meta.url,
    ),
    "utf8",
  ),
);
test("original explanations preserve independently fixed proofs and exact locations", () => {
  assert.equal(originalExplainCases(sdk, controls, testing).length, 42);
});

test("explain returns every located failure with truthful completeness", () => {
  const document = sdk.Document.parse(
    JSON.stringify({
      openbindings: "0.2.0",
      operations: {
        op: {
          input: {
            type: "object",
            properties: { a: { type: "string" }, b: { type: "integer" } },
            required: ["a", "b", "c"],
            additionalProperties: false,
          },
        },
      },
    }),
  ).value;
  const ready = prepareInput(document, "op");
  assert.equal(ready.status, "ready");
  try {
    const value = { a: 1, b: "x", z: true };
    const outcome = ready.schema.evaluate(value);
    assert.equal(outcome.status, "fails");
    assert.deepEqual(outcome.problems, []);
    assert.equal(outcome.problemsComplete, false);
    const explained = ready.schema.explain(value);
    assert.equal(explained.status, "fails");
    assert.equal(explained.problemsComplete, true);
    assert.deepEqual(
      explained.problems.map(({ instance, schema, code, details }) => [
        instance,
        schema.sourceUri,
        schema.pointer,
        code,
        details,
      ]),
      [
        ["", null, "/operations/op/input/required", "required", undefined],
        [
          "/a",
          null,
          "/operations/op/input/properties/a/type",
          "type",
          undefined,
        ],
        [
          "/b",
          null,
          "/operations/op/input/properties/b/type",
          "type",
          undefined,
        ],
        [
          "/z",
          null,
          "/operations/op/input/additionalProperties",
          "falseSchema",
          undefined,
        ],
      ],
    );
  } finally {
    ready.schema.dispose();
    document.dispose();
  }
});
