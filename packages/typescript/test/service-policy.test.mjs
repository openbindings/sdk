import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { initialize, liveStorageOwners } from "../dist/index.js";
import { prepareService, ValidationService } from "../examples/service.mjs";
await initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
function candidate(input) {
  const result = prepareService(
    JSON.stringify({
      openbindings: "0.2.0",
      operations: { lookup: { input } },
    }),
  );
  assert.equal(result.status, "ready");
  return result.contract;
}
test("service replacement requires complete resources and retains in-flight work", async () => {
  const warm = candidate(true);
  warm.dispose();
  const baseline = liveStorageOwners();
  const service = new ValidationService(candidate({ type: "integer" }));
  const partial = candidate({
    anyOf: [true, { $ref: "https://missing.invalid/U" }],
  });
  let resume, pending;
  try {
    assert.equal(partial.resourceCompleteness.status, "incomplete");
    assert.equal(partial.validate("preview").outcome, "satisfies");
    assert.throws(() => service.replace(partial), /complete schema resources/);
    assert.throws(
      () => new ValidationService(partial),
      /complete schema resources/,
    );
    assert.equal(service.check(7).outcome, "satisfies");
    pending = service.checkBytes(
      new Promise((resolve) => {
        resume = resolve;
      }),
    );
    service.replace(candidate({ type: "string" }));
    resume("7");
    assert.equal((await pending).outcome, "satisfies");
    assert.equal(service.check("new snapshot").outcome, "satisfies");
  } finally {
    resume?.("7");
    await pending;
    partial.dispose();
    service.dispose();
  }
  assert.equal(liveStorageOwners(), baseline);
});
