import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { initialize, Document, parseJson } from "../dist/index.js";
await initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);

const item = {
  type: "object",
  required: ["sku"],
  properties: { sku: { type: "string" } },
};
for (const referenced of [false, true]) {
  test(`own failures survive item reservations (${referenced ? "reference" : "inline"})`, () => {
    const parsed = Document.parse(
      JSON.stringify({
        openbindings: "0.2.0",
        operations: {
          op: {
            input: {
              type: "array",
              maxItems: 50,
              items: referenced ? { $ref: "#/schemas/Item" } : item,
            },
          },
        },
        schemas: { Item: item },
      }),
    );
    assert.equal(parsed.status, "parsed");
    let proof, schema, value;
    try {
      const checked = parsed.value.validate();
      assert.equal(checked.status, "conformant");
      proof = checked.document;
      schema = proof.inputSchema("op");
      const admitted = parseJson(`[${Array(249_000).fill(0).join(",")}]`, {
        limits: { maxBytes: 4 * 1024 * 1024, maxNodes: 250_000 },
      });
      assert.equal(admitted.status, "parsed");
      value = admitted.value;
      for (const options of [
        {},
        {
          limits: {
            evaluationSdkWorkUnits: 100_000_000,
            evaluationSdkReservationBytes: 4 * 1024 * 1024,
          },
        },
      ]) {
        const prepared = schema.prepare(options);
        assert.equal(prepared.status, "ready");
        try {
          assert.equal(prepared.schema.evaluate(value).status, "fails");
          const explained = prepared.schema.explain(value);
          assert.equal(explained.status, "fails");
          assert.equal(explained.problemsComplete, false);
          assert.ok(
            explained.problems.some(
              (p) =>
                p.code === "maxItems" &&
                p.instance === "" &&
                p.schema.pointer === "/operations/op/input/maxItems",
            ),
          );
        } finally {
          prepared.schema.dispose();
        }
      }
    } finally {
      value?.dispose();
      schema?.dispose();
      proof?.dispose();
      parsed.value.dispose();
    }
  });
}
