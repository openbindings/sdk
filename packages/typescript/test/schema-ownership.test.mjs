import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);

test("named and operation schemas retain the original document after disposal and edits", () => {
  const parsed = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{"read":{"input":{"$ref":"#/schemas/value"}}},"schemas":{"value":{"type":"integer"}}}',
  );
  assert.equal(parsed.status, "parsed");
  const original = parsed.value;
  const assessment = original.validate();
  assert.equal(assessment.status, "conformant");
  const proof = assessment.document;
  const named = proof.schema("value");
  const operation = proof.resolveOperation("read");
  const input = operation.input();
  const changed = original.set("/schemas/value", { type: "string" });
  operation.dispose();
  proof.dispose();
  original.dispose();
  for (const schema of [named, input]) {
    const prepared = schema.prepare();
    assert.equal(prepared.status, "ready");
    schema.dispose();
    try {
      assert.equal(prepared.schema.evaluate(2).status, "satisfies");
      assert.equal(prepared.schema.evaluate("two").status, "fails");
    } finally {
      prepared.schema.dispose();
    }
  }
  const newAssessment = changed.validate();
  assert.equal(newAssessment.status, "conformant");
  const newSchema = newAssessment.document.schema("value");
  newAssessment.document.dispose();
  changed.dispose();
  const prepared = newSchema.prepare();
  newSchema.dispose();
  assert.equal(prepared.status, "ready");
  try {
    assert.equal(prepared.schema.evaluate(2).status, "fails");
  } finally {
    prepared.schema.dispose();
  }
});
