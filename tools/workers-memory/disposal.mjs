// Node adaptation of the Workers lab's m7 disposal experiment. The loop is
// synchronous, as inside one request; finalizers cannot run between calls.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { docMany } from "./fixtures.mjs";

const packagePath = resolve(process.argv[2] ?? "packages/typescript");
const base = pathToFileURL(packagePath + "/dist/");
const sdk = await import(new URL("index.js", base));
await sdk.initialize(await readFile(new URL("wasm/openbindings_wasm_bg.wasm", base)));
const bridge = await import(new URL("wasm/openbindings_wasm.js", base));
const wasm = await bridge.default();
const parsed = sdk.Document.parse(docMany(20000));
assert.equal(parsed.status, "parsed");
const checked = parsed.value.validate();
assert.equal(checked.status, "conformant");
const proof = checked.document;
parsed.value.dispose();
const count = proof.operations().length;
const beforeBytes = wasm.memory.buffer.byteLength;
for (let i = 0; i < 10000; i++) proof.operations();
const afterBytes = wasm.memory.buffer.byteLength;
proof.dispose();
console.log(JSON.stringify({ node: process.version, operations: count, calls: 10000,
  beforeBytes, afterBytes, growthBytes: afterBytes - beforeBytes }));
