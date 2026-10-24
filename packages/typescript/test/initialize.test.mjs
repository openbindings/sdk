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

// A module that exports the bindings' start and runtime functions and memory,
// and nothing else the bindings call.
function impostorModule() {
  const name = (text) => [text.length, ...new TextEncoder().encode(text)];
  const section = (id, body) => [id, body.length, ...body];
  const exportFunction = (text, index) => [...name(text), 0x00, index];
  return new Uint8Array([
    ...[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    ...section(1, [1, 0x60, 0, 0]),
    ...section(3, [2, 0, 0]),
    ...section(5, [1, 0, 1]),
    ...section(7, [
      3,
      ...name("memory"),
      0x02,
      0,
      ...exportFunction("__wbindgen_start", 0),
      ...exportFunction("initializeRuntime", 1),
    ]),
    ...section(10, [2, 2, 0, 0x0b, 2, 0, 0x0b]),
  ]);
}

// Each failure runs in a fresh process, since initialization is per module
// instance, and must leave the package retryable: the next attempt with the
// package's own module initializes it and the SDK works.
const failures = {
  "rejected input promise": `Promise.reject(new Error("simulated fetch failure"))`,
  "failed fetch of a URL": `(globalThis.fetch = async () => { throw new TypeError("fetch failed"); }, "https://wasm.invalid/sdk.wasm")`,
  "unsuccessful response": `new Response("missing", { status: 404 })`,
  "invalid bytes": `new Uint8Array([9, 9, 9, 9])`,
  "empty module": `new Uint8Array([0, 0x61, 0x73, 0x6d, 1, 0, 0, 0])`,
  "impostor module bytes": `impostor`,
  "impostor compiled module": `new WebAssembly.Module(impostor)`,
  "impostor response": `new Response(impostor, { headers: { "Content-Type": "application/wasm" } })`,
};
const wrongModule = new Set([
  "empty module",
  "impostor module bytes",
  "impostor compiled module",
  "impostor response",
]);
for (const [kind, input] of Object.entries(failures))
  test(`a failed initialization (${kind}) leaves the package retryable`, async () => {
    const { spawnSync } = await import("node:child_process");
    const result = spawnSync(
      process.execPath,
      [
        "--input-type=module",
        "-e",
        `
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
const sdk = await import(${JSON.stringify(new URL("../dist/index.js", import.meta.url).href)});
const impostor = new Uint8Array(${JSON.stringify([...impostorModule()])});
const fetch = globalThis.fetch;
await assert.rejects(sdk.initialize(${input}), (error) => {
  assert.ok(error instanceof sdk.SdkError);
  assert.equal(error.code, "wasm-initialization");
  if (${wrongModule.has(kind)})
    assert.match(error.message, /the supplied module is not this package's/);
  return true;
});
globalThis.fetch = fetch;
assert.throws(() => sdk.parseJson("7"), { code: "not-initialized" });
await sdk.initialize(await readFile(new URL(${JSON.stringify(new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url).href)})));
const parsed = sdk.parseJson("7");
assert.equal(parsed.value.text, "7");
parsed.value.dispose();
`,
      ],
      { encoding: "utf8" },
    );
    assert.equal(result.status, 0, result.stderr);
  });

test("failures of every kind in turn leave the package retryable", async () => {
  const { spawnSync } = await import("node:child_process");
  const result = spawnSync(
    process.execPath,
    [
      "--input-type=module",
      "-e",
      `
import assert from "node:assert/strict";
const sdk = await import(${JSON.stringify(new URL("../dist/index.js", import.meta.url).href)});
const impostor = new Uint8Array(${JSON.stringify([...impostorModule()])});
for (const input of [
  () => new Uint8Array([0, 0x61, 0x73, 0x6d, 1, 0, 0, 0]),
  () => new WebAssembly.Module(impostor),
  () => new Uint8Array([9, 9, 9, 9]),
  () => Promise.reject(new Error("simulated fetch failure")),
  () => new Response("missing", { status: 404 }),
])
  await assert.rejects(sdk.initialize(input()), { code: "wasm-initialization" });
// Without input, Node reads the installed asset.
await sdk.initialize();
assert.equal(
  sdk.validateDocument('{"openbindings":"0.2.0","operations":{}}').report.conclusion,
  "conformant",
);
`,
    ],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
});
