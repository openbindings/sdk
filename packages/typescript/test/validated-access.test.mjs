import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import * as bridge from "../dist/wasm/openbindings_wasm.js";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const text = String.raw`{"openbindings":"0.2.0","name":"\ud800","version":"\udc00","description":"\ud800","schemas":{"t":true,"f":false,"o":{}},"operations":{"run":{"aliases":["go"],"description":"\ud800","tags":["\ud800"],"input":true,"output":false,"examples":{"e":{"description":"\ud800","input":null}}}},"sources":{"s":{"kind":"\ud800","description":"\ud800","content":null}},"dependencies":{"d":{"operation":"run","kinds":["\ud800"],"description":"\ud800"}},"bindings":{"b":{"source":"s","operation":"run","description":"\ud800"}}}`;

test("Rust proof transfer is single-use and unavailable to parsed or forged handles", () => {
  const parsed = bridge.WasmDocument.parseBytes(new TextEncoder().encode(text));
  assert.equal(typeof parsed.valueContracts, "undefined");
  const transfer = parsed.validate();
  assert.equal(JSON.parse(transfer.report()).report.conclusion, "conformant");
  const proof = transfer.takeValidated();
  assert.ok(proof instanceof bridge.WasmValidatedDocument);
  assert.equal(transfer.takeValidated(), undefined);
  transfer.free();
  parsed.free();
  const retained = proof.retain();
  proof.free();
  const resources = new bridge.WasmResources();
  const context = retained.valueContracts(resources, "{}", false);
  context.free();
  resources.free();
  retained.free();
  for (const source of [
    '{"openbindings":"0.2.0"}',
    '{"openbindings":"0.3.0","operations":{}}',
  ]) {
    const draft = bridge.WasmDocument.parseBytes(
      new TextEncoder().encode(source),
    );
    const result = draft.validate();
    assert.equal(result.takeValidated(), undefined);
    assert.equal(result.takeValidated(), undefined);
    result.free();
    draft.free();
  }
  const draft = sdk.parseDocument(text).value;
  assert.equal(typeof draft.valueContracts, "undefined");
  assert.throws(() =>
    sdk.ValidatedDocument.prototype.valueContracts.call(draft),
  );
  assert.throws(() =>
    sdk.ValidatedDocument.prototype.valueContracts.call(
      Object.create(sdk.ValidatedDocument.prototype),
    ),
  );
  // Overriding JS evidence cannot mint Rust proof.
  const invalid = sdk.parseDocument('{"openbindings":"0.2.0"}').value;
  invalid.validate = () => ({
    status: "report",
    report: { conclusion: "conformant" },
  });
  assert.equal(invalid.validated().status, "report");
  invalid.dispose();
  draft.dispose();
});

test("validated text and retained reads preserve exact UTF-16 and presence", () => {
  const draft = sdk.parseDocument(text).value;
  const result = draft.validated();
  assert.equal(result.status, "validated");
  const proof = result.document;
  assert.equal(proof.openbindings, "0.2.0");
  assert.equal(proof.name.charCodeAt(0), 0xd800);
  assert.equal(proof.version.charCodeAt(0), 0xdc00);
  assert.equal(proof.description.charCodeAt(0), 0xd800);
  const parsed = proof.parsed();
  assert.equal(parsed.name.charCodeAt(0), 0xd800);
  parsed.dispose();
  assert.equal(proof.operations[0].description.charCodeAt(0), 0xd800);
  assert.equal(proof.operations[0].tags[0].charCodeAt(0), 0xd800);
  assert.equal(proof.sources[0].kind.charCodeAt(0), 0xd800);
  assert.equal(proof.dependencies[0].kinds[0].charCodeAt(0), 0xd800);
  assert.equal(proof.bindings[0].description.charCodeAt(0), 0xd800);
  assert.equal(proof.dependencyAcceptsKind("d", "\ud800"), true);
  const schemas = proof.schemas;
  assert.deepEqual([...schemas.keys()], ["f", "o", "t"]);
  const operation = proof.resolveOperation("go").operation;
  const example = operation.example("e");
  const input = operation.input,
    output = operation.output;
  const exampleInput = example.input;
  assert.equal(example.output, undefined);
  assert.equal(example.metadata.description.charCodeAt(0), 0xd800);
  example.dispose();
  operation.dispose();
  proof.dispose();
  draft.dispose();
  assert.equal(input.text, "true");
  assert.equal(output.text, "false");
  assert.equal(exampleInput.text, "null");
  assert.deepEqual(
    [...schemas.values()].map((v) => v.text),
    ["false", "{}", "true"],
  );
  for (const value of [input, output, exampleInput, ...schemas.values()])
    value.dispose();
  for (const direction of ["input", "output"]) {
    for (const value of [undefined, "null", "true", "false", "{}"]) {
      const field = value === undefined ? "" : `,"${direction}":${value}`;
      const doc = sdk.parseDocument(
        `{"openbindings":"0.2.0","operations":{"run":{"description":"x"${field}}}}`,
      ).value;
      const op = doc.resolveOperation("run").operation;
      const acquired = op[direction];
      assert.equal(acquired?.text, value);
      assert.equal(
        op.metadata[direction === "input" ? "hasInput" : "hasOutput"],
        value !== undefined,
      );
      acquired?.dispose();
      op.dispose();
      doc.dispose();
    }
  }
  for (const [fields, expected] of [
    ["", undefined],
    [',"schemas":{}', 0],
  ]) {
    const doc = sdk.parseDocument(
      `{"openbindings":"0.2.0","operations":{}${fields}}`,
    ).value;
    const proof = doc.validated().document;
    assert.equal(proof.schemas?.size, expected);
    assert.equal(proof.name, undefined);
    proof.dispose();
    doc.dispose();
  }
});

test("undetermined admission evidence cannot produce a proof through Rust/Wasm or TypeScript", () => {
  const source =
    '{"openbindings":"0.2.0","operations":{},"x":[' +
    "0,".repeat(1_000_000) +
    "0]}";
  const bytes = new TextEncoder().encode(source);
  const raw = JSON.parse(bridge.validateBytes(bytes));
  assert.equal(raw.report.conclusion, "undetermined");
  assert.throws(() => bridge.WasmDocument.parseBytes(bytes));
  const result = sdk.validateDocument(bytes);
  assert.equal(result.status, "report");
  assert.equal(result.report.conclusion, "undetermined");
  assert.equal(sdk.parseDocument(bytes).status, "input-error");
  assert.throws(() =>
    sdk.ValidatedDocument.prototype.valueContracts.call(result),
  );
});
