import test from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const MIB = 1024 * 1024;
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

test("hostile copied identifiers finish within public document and finding limits", async (t) => {
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
  t.diagnostic(
    JSON.stringify(
      results.map((result, at) => ({
        name: shapes[at][0],
        ...result,
      })),
    ),
  );
  results.forEach((result, at) => {
    const [name, , conclusion] = shapes[at];
    assert.equal(result.conclusion, conclusion, name);
    assert(result.bytes <= 32 * MIB, "document fits the public byte limit");
  });
  assert.equal(results[1].findings, 4096);
});

// A refusal on length needs only the prefix that locates it, so module memory
// does not grow by the size of an input far past the byte limit. A fresh process
// keeps the measured growth this test's own.
test("input far past the byte limit does not grow module memory by its size", async () => {
  const program = `
import { readFile } from "node:fs/promises";
const url = (path) => new URL(path, ${JSON.stringify(import.meta.url)});
const sdk = await import(url("../dist/index.js"));
const bridge = await import(url("../dist/wasm/openbindings_wasm.js"));
await sdk.initialize(await readFile(url("../dist/wasm/openbindings_wasm_bg.wasm")));
const { memory } = await bridge.default();
const size = 64 * 1024 * 1024, limits = { maxBytes: 1024 * 1024 };
const bytes = new Uint8Array(size).fill(0x20), text = " ".repeat(size);
const before = memory.buffer.byteLength;
const outcomes = [
  sdk.parseJson(bytes, { limits }).status,
  sdk.parseJson(text, { limits }).status,
  sdk.Document.parse(bytes, { limits }).status,
  sdk.validateDocument(text, { limits }).report.conclusion,
];
console.log(JSON.stringify({ added: memory.buffer.byteLength - before, outcomes }));
`;
  const { stdout } = await run(
    process.execPath,
    ["--input-type=module", "-e", program],
    { encoding: "utf8" },
  );
  const { added, outcomes } = JSON.parse(stdout);
  assert.deepEqual(outcomes, [
    "input-error",
    "input-error",
    "input-error",
    "undetermined",
  ]);
  assert.ok(added < 16 * MIB, `module memory grew by ${added} bytes`);
});
