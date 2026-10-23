import test from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const MIB = 1024 * 1024;
// Wasm linear memory only grows, so its size after a call is the call's
// high-water mark, the module's copy of the input included. For one build it is
// the same on every run, so each document's bound is what its validation adds
// now, 223.75 and 227.75 MiB, rounded up to the next MiB: any growth fails.
// Lower a bound when its figure falls; raise one only for a reason recorded
// beside it. The diagnostic memory probe allows 300 MiB of native requested
// memory for the same documents (tools/diagnostic-memory-probe/src/limits.rs).
const run = promisify(execFile);

// The probe's costliest documents at the default 32 MiB limit, byte for byte:
// a third of a million escaped `$id`s, each with its own authority and a dot
// segment that makes its path a copy, alone and with a query and fragment.
// Each is validated in a fresh process, so the module memory measured is this
// call's alone.
const child = (pieces) => `
import { readFile } from "node:fs/promises";
const url = (path) => new URL(path, ${JSON.stringify(import.meta.url)});
const sdk = await import(url("../dist/index.js"));
const bridge = await import(url("../dist/wasm/openbindings_wasm.js"));
await sdk.initialize(await readFile(url("../dist/wasm/openbindings_wasm_bg.wasm")));
const { memory } = await bridge.default();
const count = 333_320, limit = 32 * 1024 * 1024, pieces = ${JSON.stringify(pieces)};
const element = (i, n) => '{"$id":"s:\\\\/\\\\/' + i.toString(16) + "/q/a/../b/" + "c".repeat(n) + "/" + pieces + '"}';
const prefix = '{"openbindings":"0.2.0","operations":{},"schemas":{"s":{"allOf":[';
const suffix = "]}}}";
let base = prefix.length + suffix.length;
for (let i = 0; i < count; i++) base += element(i, 0).length + 1;
const length = Math.floor((limit - base) / count);
const parts = [];
for (let i = 0; i < count; i++) parts.push(element(i, length));
const bytes = new TextEncoder().encode(prefix + parts.join(",") + suffix);
const before = memory.buffer.byteLength;
const result = sdk.validateDocument(bytes);
console.log(JSON.stringify({
  bytes: bytes.length,
  added: memory.buffer.byteLength - before,
  conclusion: result.report.conclusion,
  findings: result.report.findings.length,
}));
`;

test("validating the probe's costliest documents adds no more module memory than it does now", async () => {
  const shapes = [
    ["copied identifier paths", "", "conformant", 224 * MIB],
    ["copied identifier paths with pieces", "?#f", "non-conformant", 228 * MIB],
  ];
  const results = await Promise.all(
    shapes.map(async ([, pieces]) => {
      const { stdout } = await run(
        process.execPath,
        ["--input-type=module", "-e", child(pieces)],
        { encoding: "utf8" },
      );
      return JSON.parse(stdout);
    }),
  );
  results.forEach((result, at) => {
    const [name, , conclusion, bound] = shapes[at];
    assert.equal(result.conclusion, conclusion, name);
    assert(result.bytes <= 32 * MIB, `${name}: ${result.bytes} bytes`);
    assert(
      result.added <= bound,
      `${name}: the call added ${(result.added / MIB).toFixed(2)} MiB of module memory, more than its ${bound / MIB} MiB bound`,
    );
  });
  assert.equal(results[1].findings, 4096);
});
