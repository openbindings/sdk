// DOC-1.1: the independent review's proof-mutation.mjs attack, plus raw Wasm receivers.
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
  proof.valueContracts().dispose();
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
    ["edit", ['[{"op":"set","pointer":"/name","value":7}]']],
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
