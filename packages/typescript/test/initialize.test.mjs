import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";

test("initialization rejects unavailable entropy and invalid modules, then permits healthy retry", async () => {
  const bytes = await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  );
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, "crypto");
  try {
    Object.defineProperty(globalThis, "crypto", {
      configurable: true,
      value: {
        getRandomValues() {
          throw new Error("request required");
        },
      },
    });
    await assert.rejects(
      sdk.initialize(bytes),
      (error) =>
        error instanceof sdk.SdkError &&
        error.code === "host-entropy-unavailable",
    );
    assert.throws(() => sdk.parseJson("7"), { code: "not-initialized" });
  } finally {
    if (descriptor) Object.defineProperty(globalThis, "crypto", descriptor);
    else delete globalThis.crypto;
  }
  await assert.rejects(sdk.initialize(new Uint8Array([0])), {
    code: "wasm-initialization",
  });
  const first = sdk.initialize(bytes);
  assert.equal(first, sdk.initialize(bytes));
  await first;
  const document = sdk.parseDocument(
    '{"openbindings":"0.2.0","operations":{}}',
  );
  assert.equal(document.status, "parsed");
  assert.equal(document.value.assess().report.conclusion, "conformant");
  document.value.dispose();
});
