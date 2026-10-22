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
    ["setName", ["changed"]],
    ["setOperations", [{}]],
    ["setOperation", ["run", {}]],
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
    ["setName", ["changed"]],
    ["setVersion", ["changed"]],
    ["setDescription", ["changed"]],
    ["setOperationDescription", ["run", "changed"]],
    ["setOperationInput", ["run", value]],
    ["setOperationOutput", ["run", value]],
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
  );
  assert.equal(JSON.parse(result[0]).code, "immutable-document");
  assert.deepEqual(proof.toBytes(), before);
  assert.equal(JSON.parse(proof.name()), "certified");
  const independent = proof.asDocument();
  independent.setName("independent");
  assert.deepEqual(proof.toBytes(), before);
  independent.free();
  value.free();
  proof.free();
  validation.free();
  document.free();
});

test("a plain operation view cannot be borrowed as a conformant one to hand out contracts", () => {
  const document = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{"run":{"input":{"type":"integer"}}}}',
  ).value;
  const selection = document.resolveOperation("run");
  assert.equal(selection.status, "found");
  const plain = selection.operation;
  const conformance = (error) =>
    error instanceof sdk.SdkError && error.code === "document-conformance";
  for (const method of ["input", "output"])
    assert.throws(
      () => sdk.ConformantOperationView.prototype[method].call(plain),
      conformance,
    );
  const input = plain.input();
  for (const method of ["schema", "prepare", "retain"])
    assert.throws(
      () => sdk.ValueContract.prototype[method].call(input),
      conformance,
    );
  input.dispose();
  plain.dispose();
  document.dispose();
});
