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
  const input = proof.inputSchema(operation.key);
  const changed = original.set("/schemas/value", { type: "string" });

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

import { liveStorageOwners } from "../dist/testing.js";

test("a schema or exact leaf pins the whole document arena after both roots are disposed", () => {
  // Warm fixed-schema storage before comparing document arenas.
  sdk.validateDocument('{"openbindings":"0.2.0","operations":{}}');
  const baseline = liveStorageOwners();
  for (const kind of ["schema", "exact"]) {
    const document = sdk.Document.parse(
      JSON.stringify({
        openbindings: "0.2.0",
        operations: { run: { input: true } },
        "x-unrelated": "x".repeat(1024 * 1024),
      }),
    ).value;
    const proof = document.validate().document;
    const record = proof.operation("run");
    assert.ok(document.retainedBytes() > 1024 * 1024);
    assert.equal(liveStorageOwners(), baseline + 1);
    let held;
    if (kind === "schema") held = proof.inputSchema(record.key);
    else {
      const declaration = proof.operationValue(record.key);
      held = declaration.get("input");
      declaration.dispose();
    }
    proof.dispose();
    document.dispose();
    // The tiny owner shares the original arena, including x-unrelated, rather
    // than a copied subtree. Entry metadata alone would retain no arena.
    assert.equal(liveStorageOwners(), baseline + 1, kind);
    const exact = kind === "schema" ? held.json() : held.retain();
    assert.equal(exact.text, "true");
    exact.dispose();
    held.dispose();
    assert.equal(liveStorageOwners(), baseline, kind);
    assert.equal(record.key, "run");
  }
});
