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
  const document = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{}}',
  );
  assert.equal(document.status, "parsed");
  assert.equal(document.value.validate().report.conclusion, "conformant");
  document.value.dispose();
});

test("browser default initialization still fetches the module-relative asset", async () => {
  const { spawnSync } = await import("node:child_process");
  const result = spawnSync(
    process.execPath,
    [
      "--input-type=module",
      "-e",
      `
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
const asset = new URL("../dist/wasm/openbindings_wasm_bg.wasm", ${JSON.stringify(import.meta.url)});
const bytes = await readFile(asset);
const sdk = await import(new URL("../dist/index.js", ${JSON.stringify(import.meta.url)}));
delete globalThis.process;
let fetches = 0;
globalThis.fetch = async (url) => {
  fetches++;
  assert.equal(String(url), asset.href);
  return new Response(bytes, { headers: { "Content-Type": "application/wasm" } });
};
await sdk.initialize();
assert.equal(fetches, 1);
const parsed = sdk.parseJson("7");
assert.equal(parsed.value.text, "7");
parsed.value.dispose();
`,
    ],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
});
