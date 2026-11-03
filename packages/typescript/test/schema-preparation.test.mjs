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
function document(schema) {
  const parsed = sdk.Document.parse(
    JSON.stringify({
      openbindings: "0.2.0",
      operations: { run: { input: schema } },
    }),
  );
  assert.equal(parsed.status, "parsed");
  return parsed.value;
}
test("preparation failures remain distinct from owned instance outcomes", () => {
  const doc = document(true);
  const cancellation = new AbortController();
  cancellation.abort();
  const refused = prepareInput(doc, "run", { signal: cancellation.signal });
  assert.deepEqual(refused, {
    status: "refused",
    failure: { kind: "cancelled", origin: null },
  });
  const ready = prepareInput(doc, "run");
  assert.equal(ready.status, "ready");
  doc.dispose();
  const prepared = ready.schema;
  try {
    assert.equal(prepared.evaluate(7).status, "satisfies");
    assert.equal(
      prepared.evaluateJson("null", { signal: cancellation.signal }).detail
        .reason,
      "cancelled",
    );
    assert.equal(
      prepared.evaluateJson("{", { signal: cancellation.signal }).status,
      "input-error",
    );
    assert.equal(
      prepared.evaluateJson('{"x":1,"x":2}').detail.code,
      "duplicate-instance-members",
    );
    assert.equal("validate" in prepared, false);
    assert.equal("valueContract" in ready, false);
  } finally {
    prepared.dispose();
  }
});
test("unavailable annotation content remains a per-value inability without invented provenance", () => {
  const doc = document({
    $ref: "https://missing.test/schema",
    unevaluatedProperties: false,
  });
  let prepared;
  try {
    const setup = prepareInput(doc, "run");
    assert.equal(setup.status, "ready");
    prepared = setup.schema;
    const result = prepared.evaluate({ x: 1 });
    assert.equal(result.status, "no-verdict");
    assert.equal(result.detail.reason, "resource-unavailable");
    assert.equal(result.detail.location, null);
  } finally {
    prepared?.dispose();
    doc.dispose();
  }
});
