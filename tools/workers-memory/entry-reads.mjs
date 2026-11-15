// Built-package timings; each process measures one shape without other work.
// No metadata cache is warmed for the cold cases. Parsing and validation are
// outside the timed interval. Disposal is timed for acquired owners.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const [packagePath = "packages/typescript", count = "1000", projection = "full"] = process.argv.slice(2);
const n = Number(count);
assert.ok(["full", "scalars"].includes(projection));
const base = pathToFileURL(resolve(packagePath) + "/dist/");
const sdk = await import(new URL("index.js", base));
await sdk.initialize(await readFile(new URL("wasm/openbindings_wasm_bg.wasm", base)));
const operations = Object.fromEntries(Array.from({ length: n }, (_, i) => [
  `op${String(i).padStart(5, "0")}`, { description: "an operation",
    ...(projection === "full" ? { aliases: [`alias${i}`], tags: ["second", "first"] } : {}),
    deprecated: false, input: { type: "string" } },
]));
const parsed = sdk.Document.parse(JSON.stringify({ openbindings: "0.2.0", operations }));
assert.equal(parsed.status, "parsed");
const checked = parsed.value.validate();
assert.equal(checked.status, "conformant");
const proof = checked.document;
const detached = typeof proof.inputSchema === "function";
const key = `op${String(Math.floor(n / 2)).padStart(5, "0")}`;
let sink = 0;
const cases = {
  operations(p) {
    const entries = p.operations();
    for (const entry of entries) sink += entry.key.length;
    if (!detached) for (const entry of entries) entry.dispose();
  },
  operation(p) {
    const entry = p.operation(key);
    if (!detached) entry.dispose();
  },
  operationField(p) {
    const entry = p.operation(key);
    sink += entry.key.length;
    if (!detached) entry.dispose();
  },
  inputSchema(p) {
    if (detached) p.inputSchema(key).dispose();
    else {
      const entry = p.operation(key);
      entry.input().dispose();
      entry.dispose();
    }
  },
};
const results = {};
for (const [name, run] of Object.entries(cases)) {
  const iterations = name === "operations" ? Math.max(10, Math.floor(20000 / n)) : 20000;
  for (let i = 0; i < Math.min(iterations, 1000); i++) run(proof);
  const warm = [], cold = [];
  for (let sample = 0; sample < 11; sample++) {
    globalThis.gc?.();
    let start = performance.now();
    for (let i = 0; i < iterations; i++) run(proof);
    warm.push((performance.now() - start) * 1e6 / iterations);
    // A fresh proof wrapper on every read defeats any facade memoization.
    let elapsed = 0;
    for (let i = 0; i < Math.min(iterations, 1000); i++) {
      const fresh = proof.retain();
      start = performance.now();
      run(fresh);
      elapsed += performance.now() - start;
      fresh.dispose();
    }
    cold.push(elapsed * 1e6 / Math.min(iterations, 1000));
  }
  const median = values => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
  results[name] = { warmNs: median(warm), coldNs: median(cold), warm, cold };
}
proof.dispose();
parsed.value.dispose();
console.log(JSON.stringify({ node: process.version, n, projection, detached, sink, results }));
