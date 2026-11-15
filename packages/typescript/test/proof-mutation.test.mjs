// Proof mutation and raw Wasm receivers cannot bypass document conformance.
import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
import * as bridge from "../dist/wasm/openbindings_wasm.js";
await sdk.initialize();
const text = '{"openbindings":"0.2.0","name":"certified","operations":{}}';

test("runtime-private edit helpers cannot expose or mutate a proof's revision", () => {
  const document = sdk.Document.parse(text).value;
  const proof = document.validate().document;
  const before = proof.toBytes();
  let edit;
  assert.throws(
    () =>
      sdk.Document.prototype.commit.call(document, (raw) => {
        edit = raw.edit;
      }),
    TypeError,
  );
  assert.throws(
    () =>
      sdk.Document.prototype.commit.call(proof, (raw) =>
        edit.call(raw, '[{"op":"set","pointer":"/name","value":7}]'),
      ),
    TypeError,
  );
  for (const method of ["commit", "encoded", "part", "parts"])
    assert.equal(sdk.Document.prototype[method], undefined);
  for (const [method, args] of [
    ["edit", [[{ op: "set", pointer: "/name", value: 7 }]]],
    ["set", ["/name", 7]],
    ["remove", ["/name"]],
  ])
    assert.throws(() => sdk.Document.prototype[method].call(proof, ...args));
  assert.deepEqual(proof.toBytes(), before);
  assert.equal(proof.name, "certified");
  assert.equal(proof.disposed, false);
  assert.equal(proof.resolveOperation("absent"), undefined);
  proof.dispose();
  document.dispose();
});

test("Wasm proof storage refuses every borrowed editing entry", () => {
  const document = bridge.WasmDocument.parseBytes(
    new TextEncoder().encode(text),
  );
  const validation = document.validate();
  const proof = validation.takeConformant();
  const value = bridge.WasmJson.parseText("7");
  const before = proof.toBytes();
  const immutable = (error) => JSON.parse(error).code === "immutable-document";
  for (const [method, args] of [
    ["set", ["/name", value]],
    ["remove", ["/name"]],
  ])
    assert.throws(
      () => bridge.WasmDocument.prototype[method].call(proof, ...args),
      immutable,
    );
  const result = bridge.WasmDocument.prototype.editEncoded.call(
    proof,
    ['[{"op":"set","pointer":"/name","value":7}]'],
    new Uint32Array(),
    [],
    new Uint32Array([0, 0]),
  );
  assert.equal(JSON.parse(result[0]).code, "immutable-document");
  assert.deepEqual(proof.toBytes(), before);
  assert.equal(JSON.parse(proof.name()), "certified");
  const independent = proof.asDocument();
  const replacement = bridge.WasmJson.parseText('"independent"');
  independent.set("/name", replacement);
  replacement.free();
  assert.deepEqual(proof.toBytes(), before);
  independent.free();
  value.free();
  proof.free();
  validation.free();
  document.free();
});

test("raw JSON and detached records cannot be borrowed to produce schemas", () => {
  const document = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{"run":{"input":true,"examples":{"e":{"input":42}}}}}',
  ).value;
  const proof = document.validate().document;
  const operation = proof.operation("run"),
    example = proof.example(operation.key, "e");
  const raw = document.value(),
    input = raw.at("/operations/run/input");
  for (const receiver of [raw, input, example, operation]) {
    const refused = (error) =>
      error instanceof sdk.SdkError &&
      error.code ===
        ([raw, input].includes(receiver)
          ? "document-conformance"
          : "disposed-handle");
    for (const method of ["inputSchema", "outputSchema"])
      assert.throws(
        () => sdk.ConformantDocument.prototype[method].call(receiver, "run"),
        refused,
      );
    for (const method of ["json", "prepare", "retain"])
      assert.throws(() => sdk.Schema.prototype[method].call(receiver), refused);
  }
  for (const owner of [input, raw, proof, document]) owner.dispose();
});

test("every canonical Wasm query checks proof authority before borrowed reads", () => {
  const draft = bridge.WasmDocument.parseText(
    '{"openbindings":"0.2.0","operations":{"op":{"input":true,"examples":{"e":{"input":42}}}},"sources":{"s":{"kind":"k"}},"bindings":{"b":{"operation":"op","source":"s"}},"dependencies":{"d":{"operation":"op"}}}',
  );
  const assessment = draft.validate(),
    proof = assessment.takeConformant();
  const invalid = bridge.WasmDocument.parseText('{"openbindings":"0.2.0"}');
  const transfer = invalid.validate();
  const reads = [
    ["operations", []],
    ["operation", ["op", "op"]],
    ["resolveOperation", ["op"]],
    ["bindings", []],
    ["binding", ["b"]],
    ["sources", []],
    ["source", ["s"]],
    ["dependencies", []],
    ["dependency", ["d"]],
    ["inputSchema", ["op"]],
    ["outputSchema", ["op"]],
    ["operationBindings", ["op"]],
    ["examples", ["op"]],
    ["example", ["op", "e"]],
    ["operationValue", ["op"]],
    ["bindingValue", ["b"]],
    ["sourceValue", ["s"]],
    ["dependencyValue", ["d"]],
    ["exampleValue", ["op", "e"]],
    ["sourceData", ["s"]],
    ["bindingData", ["b"]],
    ["exampleInput", ["op", "e"]],
    ["exampleOutput", ["op", "e"]],
  ];
  try {
    const refused = (error) =>
      JSON.parse(error).code === "document-conformance";
    for (const receiver of [invalid, transfer])
      for (const [method, args] of reads)
        assert.throws(
          () =>
            bridge.WasmConformantDocument.prototype[method].call(
              receiver,
              ...args,
            ),
          refused,
          method,
        );
    const schema = proof.inputSchema("op"),
      json = schema.json();
    assert.equal(json.text(), "true");
    json.free();
    schema.free();
    const value = proof.exampleInput("op", "e");
    assert.equal(value.text(), "42");
    value.free();
    assert.deepEqual(proof.operation("op", "op"), { key: "op" });
  } finally {
    transfer.free();
    invalid.free();
    proof.free();
    assessment.free();
    draft.free();
  }
});
