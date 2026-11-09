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
const vocabulary = "https://json-schema.org/draft/2020-12/vocab/";
function prepare(consumer, { restriction = {}, requiredFormat = true } = {}) {
  const meta = sdk.parseJson(
    JSON.stringify({
      $schema: "https://json-schema.org/draft/2020-12/schema",
      $vocabulary: {
        [vocabulary + "core"]: true,
        [vocabulary + "validation"]: true,
        [vocabulary + "format-assertion"]: requiredFormat,
      },
      ...restriction,
    }),
  ).value;
  const supplied = sdk.parseJson(
    JSON.stringify({ $schema: "https://host.test/meta", ...consumer }),
  ).value;
  const resources = new sdk.ExternalSchemas([
    ["https://host.test/meta", meta],
    ["https://host.test/consumer", supplied],
  ]);
  const document = sdk.Document.parse(
    JSON.stringify({
      openbindings: "0.2.0",
      operations: { op: { input: { $ref: "https://host.test/consumer" } } },
    }),
  ).value;
  try {
    return prepareInput(document, "op", { resources });
  } finally {
    document.dispose();
    resources.dispose();
    supplied.dispose();
    meta.dispose();
  }
}

test("custom meta admission and retained validators survive caller disposal", () => {
  const denied = prepare({}, { restriction: { required: ["marker"] } });
  assert.equal(denied.status, "refused");
  assert.equal(denied.failure.kind, "invalid-external-schema");
  const allowed = prepare(
    { marker: true, type: [] },
    { restriction: { required: ["marker"] } },
  );
  assert.equal(allowed.status, "ready");
  try {
    for (const value of [null, false, 1, "text", [], {}]) {
      assert.equal(allowed.schema.evaluate(value).status, "fails");
    }
  } finally {
    allowed.schema.dispose();
  }
});
