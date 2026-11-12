import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";

await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);

function resourceFailure(action) {
  let failure;
  assert.throws(action, (error) => {
    if (!(error instanceof sdk.SdkError) || error.code !== "resource")
      return false;
    failure = { code: error.code, message: error.message };
    return true;
  });
  return failure;
}

test("resource staging stops at the first count or URI byte overflow", () => {
  const parsed = sdk.parseJson("true");
  assert.equal(parsed.status, "parsed");
  const schema = parsed.value;
  const overIdentityBudget = "urn:" + "a".repeat(8 * 1024 * 1024 - 4);
  const prefix = new sdk.ExternalSchemas([["urn:first", schema]]);
  try {
    function* tooManyBytes() {
      yield ["urn:first", schema];
      yield [overIdentityBudget, schema];
      throw new Error("iteration continued after the URI byte limit");
    }
    const bytes = resourceFailure(
      () => new sdk.ExternalSchemas(tooManyBytes()),
    );
    assert.match(bytes.message, /max_identity_bytes/);
    // add() calls Rust registration on the same prefix followed by this entry.
    assert.deepEqual(
      bytes,
      resourceFailure(() => prefix.with(overIdentityBudget, schema)),
    );
    function* tooMany() {
      for (let i = 0; i <= 100_000; i++) yield [`urn:s${i}`, schema];
      throw new Error("iteration continued after the resource count limit");
    }
    const count = resourceFailure(() => new sdk.ExternalSchemas(tooMany()));
    assert.match(count.message, /max_schema_nodes/);
    new sdk.ExternalSchemas([["urn:healthy", schema]]).dispose();
  } finally {
    prefix.dispose();
    schema.dispose();
  }
});

test("resource construction preserves invalid and duplicate identity errors", () => {
  const parsed = sdk.parseJson("true");
  assert.equal(parsed.status, "parsed");
  const schema = parsed.value;
  const first = "https://schema.test/id";
  const resources = new sdk.ExternalSchemas([[first, schema]]);
  try {
    for (const [uri, message] of [
      ["relative", /absolute URI/],
      ["https://schema.test/a/../id#", /identity is repeated/],
    ]) {
      let completed = false;
      function* entries() {
        yield [first, schema];
        yield [uri, schema];
        completed = true;
      }
      const failure = resourceFailure(() => new sdk.ExternalSchemas(entries()));
      assert.equal(completed, true);
      assert.match(failure.message, message);
      assert.deepEqual(
        failure,
        resourceFailure(() => resources.with(uri, schema)),
      );
    }
  } finally {
    resources.dispose();
    schema.dispose();
  }
});
