import { liveStorageOwners } from "../dist/testing.js";
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { initialize } from "../dist/index.js";
import {
  prepareService,
  ValidationService,
  readBody,
  jsonLimits,
  responseStatus,
} from "../examples/service.mjs";
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
  return result.schema;
}
test("service selects the active schema after body I/O and handles partial contracts", async () => {
  const warm = candidate(true);
  warm.dispose();
  const baseline = liveStorageOwners();
  const service = new ValidationService(candidate({ type: "integer" }));
  const partial = candidate({
    anyOf: [true, { $ref: "https://missing.invalid/U" }],
  });
  let resume, pending;
  try {
    assert.equal(partial.evaluate("preview").status, "satisfies");
    service.replace(partial.retain());
    assert.equal(service.check("preview").status, "satisfies");
    service.replace(candidate({ type: "integer" }));
    assert.equal(service.check(7).status, "satisfies");
    const beforeBody = liveStorageOwners();
    pending = service.checkBytes(
      new Promise((resolve) => {
        resume = resolve;
      }),
    );
    service.replace(candidate({ type: "string" }));
    assert.equal(liveStorageOwners(), beforeBody);
    resume("7");
    assert.equal((await pending).status, "fails");
    assert.equal(service.check("new snapshot").status, "satisfies");
  } finally {
    resume?.("7");
    await pending;
    partial.dispose();
    service.dispose();
  }
  assert.equal(liveStorageOwners(), baseline);
});

test("streamed bodies stop at the host byte bound", async () => {
  const bound = jsonLimits.maxBytes;
  let cancelled = false;
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(new Uint8Array(bound));
      controller.enqueue(new Uint8Array(1));
    },
    cancel() {
      cancelled = true;
    },
  });
  await assert.rejects(readBody(stream), RangeError);
  assert.equal(cancelled, true);
  const exact = new ReadableStream({
    start(controller) {
      controller.enqueue(new Uint8Array(bound));
      controller.close();
    },
  });
  assert.equal((await readBody(exact)).byteLength, bound);
});

test("request boundaries distinguish undecided and invalid values", () => {
  assert.equal(responseStatus({ status: "satisfies" }), 200);
  assert.equal(responseStatus({ status: "fails" }), 422);
  assert.equal(responseStatus({ status: "input-error" }), 400);
  assert.equal(responseStatus({ status: "no-verdict" }), 503);
  assert.equal(responseStatus({ status: "undefined" }), 503);
});
