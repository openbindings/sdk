import test from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const MIB = 1024 * 1024;
// The diagnostic memory probe's allowance for one validate_document call at
// the default limits (tools/diagnostic-memory-probe/src/limits.rs), held here
// to the module memory the same call adds through the package. Wasm linear
// memory only grows, so its size after the call is the call's high-water mark,
// and it includes the module's copy of the input.
const ALLOWANCE = 300 * MIB;
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

test("validating the probe's costliest documents adds at most its allowance of module memory", async () => {
  const shapes = [
    ["copied identifier paths", "", "conformant"],
    ["copied identifier paths with pieces", "?#f", "non-conformant"],
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
    const [name, , conclusion] = shapes[at];
    assert.equal(result.conclusion, conclusion, name);
    assert(result.bytes <= 32 * MIB, `${name}: ${result.bytes} bytes`);
    assert(
      result.added <= ALLOWANCE,
      `${name}: the call added ${(result.added / MIB).toFixed(1)} MiB of module memory, more than ${ALLOWANCE / MIB} MiB`,
    );
  });
  assert.equal(results[1].findings, 4096);
});
