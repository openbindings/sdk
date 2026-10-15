// DOC-1: document states, native atomic editing and proof ownership.
import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
import * as bridge from "../dist/wasm/openbindings_wasm.js";
import { liveStorageOwners } from "../dist/testing.js";
await sdk.initialize();
const open = (text) => {
  const result = sdk.Document.parse(text);
  assert.equal(result.status, "parsed");
  return result.value;
};
const exact = (text) => {
  const result = sdk.parseJson(text);
  assert.equal(result.status, "parsed");
  return result.value;
};
const text =
  ' {\n"openbindings":"0.2.0", "operations":{"run":{"input":true}},\n "x-number":1e+003, "x-object": { "untouched" : -0 }\n} ';

test("proof constructors and plain-object forgeries cannot grant authority", () => {
  const document = open(text);
  assert.throws(() => new sdk.ConformantDocument(), TypeError);
  assert.throws(
    () =>
      Reflect.construct(sdk.ConformantDocument, [
        {},
        Symbol("established conformance"),
      ]),
    TypeError,
  );
  const forged = Object.create(sdk.ConformantDocument.prototype);
  assert.throws(
    () => forged.valueContracts(),
    (e) => e.code === "disposed-handle",
  );
  assert.throws(
    () => sdk.ConformantDocument.prototype.valueContracts.call(document),
    (e) => e.code === "document-conformance",
  );
  const proof = document.validate().document;
  assert.ok(proof instanceof sdk.ConformantDocument);
  assert.equal(proof instanceof sdk.Document, false);
  for (const method of [
    "set",
    "remove",
    "edit",
    "setOperation",
    "setDescription",
  ])
    assert.equal(proof[method], undefined);
  assert.equal(document.valueContracts, undefined);
  document.dispose();
  proof.dispose();
});

test("consuming proof revokes all aliases while retained proofs and editable copies survive", () => {
  const document = open(text),
    proof = document.validate().document,
    alias = proof,
    retained = proof.retain(),
    independent = proof.asDocument();
  const initial = proof.toBytes();
  independent.setDescription("different revision");
  assert.deepEqual(proof.toBytes(), initial);
  const editable = proof.intoDocument();
  assert.equal(alias.disposed, true);
  for (const read of [
    () => alias.operations,
    () => alias.toBytes(),
    () => alias.value(),
    () => alias.valueContracts(),
    () => alias.asDocument(),
    () => alias.intoDocument(),
  ])
    assert.throws(read, (e) => e.code === "disposed-handle");
  editable.setOperationDescription("run", "editing again");
  assert.deepEqual(retained.toBytes(), initial);
  assert.equal(retained.operations[0].key, "run");
  for (const owner of [document, proof, retained, independent, editable])
    owner.dispose();
});

test("one batch commits once and preserves decimal spelling and every untouched byte", () => {
  const document = open(text),
    decimal = exact("0.29000000000000001");
  const original = bridge.WasmDocument.prototype.editEncoded;
  let calls = 0;
  const read = bridge.WasmJson.prototype.text;
  bridge.WasmJson.prototype.text = () => {
    throw new Error("unexpected exact-value read across the bridge");
  };
  bridge.WasmDocument.prototype.editEncoded = function (...args) {
    calls++;
    return original.apply(this, args);
  };
  try {
    document.edit([
      {
        op: "set",
        pointer: "/operations/run/input",
        value: { const: decimal },
      },
      { op: "set", pointer: "/description", value: "edited" },
    ]);
    assert.equal(calls, 1);
    assert.equal(
      new TextDecoder().decode(document.toBytes()),
      text
        .replace('"input":true', '"input":{"const":0.29000000000000001}')
        .replace("\n} ", '\n,"description":"edited"} '),
    );
    bridge.WasmJson.prototype.text = read;
    assert.equal(decimal.text, "0.29000000000000001");
  } finally {
    bridge.WasmJson.prototype.text = read;
    bridge.WasmDocument.prototype.editEncoded = original;
    document.dispose();
    decimal.dispose();
  }
});

test("failed batches preserve bytes, metadata caches and retained evidence", () => {
  const document = open(text),
    operations = document.operations,
    proof = document.validate().document,
    before = document.toBytes();
  assert.throws(
    () =>
      document.edit([
        { op: "set", pointer: "/name", value: "new" },
        { op: "remove", pointer: "/missing" },
      ]),
    (e) => e instanceof sdk.EditError && e.code === "missing-value",
  );
  assert.deepEqual(document.toBytes(), before);
  assert.equal(document.operations, operations);
  document.edit([
    { op: "set", pointer: "/operations/run/description", value: "changed" },
  ]);
  assert.notEqual(document.operations, operations);
  assert.deepEqual(proof.toBytes(), before);
  assert.equal(document.operations[0].description, "changed");
  proof.dispose();
  document.dispose();
});

test("coalesced insertions, escaped keys and all typed part setters construct one document", () => {
  const document = sdk.Document.create();
  document.edit(
    Array.from({ length: 1000 }, (_, i) => ({
      op: "set",
      pointer: `/operations/op${i}`,
      value: {},
    })),
  );
  assert.equal(document.operations.length, 1000);
  document.setOperation("run/~", { input: true });
  document.setSource("s/~", { kind: "custom", content: null });
  document.setBinding("b/~", {
    operation: "run/~",
    source: "s/~",
    preference: 0,
  });
  document.setDependency("d/~", { operation: "run/~" });
  document.setExample("run/~", "e/~", { input: null });
  document.setOperations({ run: { input: true } });
  document.setSources({ s: { kind: "custom", content: null } });
  document.setBindings({ b: { operation: "run", source: "s" } });
  document.setDependencies({ d: { operation: "run" } });
  document.setExample("run", "e", { input: null });
  const checked = document.validate();
  assert.equal(checked.status, "conformant");
  checked.document.dispose();
  const before = document.toBytes();
  assert.throws(
    () => document.setOperation("bad/~", { additionalFields: { input: true } }),
    (e) =>
      e.code === "field-collision" &&
      e.partPointer === "/operations/bad~1~0/additionalFields/input" &&
      !Object.hasOwn(e, "draftPointer"),
  );
  assert.deepEqual(document.toBytes(), before);
  document.dispose();
});

test("duplicate documents remain inspectable and immutable until replaced by a distinct document", () => {
  const text = '{"openbindings":"0.2.0","operations":{},"x":1,"x":2}',
    document = open(text);
  assert.equal(document.validate().status, "report");
  assert.throws(
    () => document.operations,
    (e) => e.interpretationCode === "duplicate-members",
  );
  assert.throws(
    () => document.setName("refused"),
    (e) => e.code === "duplicate-members",
  );
  assert.equal(new TextDecoder().decode(document.toBytes()), text);
  document.dispose();
});

test("typed text setters preserve lone UTF-16 and source positions follow the current revision", () => {
  const document = open(text);
  document.setName("\ud800");
  document.setDescription("\udc00");
  document.setOperationDescription("run", "\ud800");
  assert.equal(document.name, "\ud800");
  assert.equal(document.description, "\udc00");
  document.set("/operations", null);
  const bytes = document.toBytes();
  assert.throws(
    () => document.operations,
    (e) =>
      new TextDecoder().decode(
        bytes.subarray(e.location.byteOffset, e.location.byteOffset + 4),
      ) === "null",
  );
  document.dispose();
});

test("released edited owners and consumed proofs release their arenas", () => {
  const warm = open(text);
  warm.validate().document.dispose();
  warm.dispose();
  const baseline = liveStorageOwners();
  const document = open(text),
    value = exact("1e+003");
  document.set("/x-number", value);
  const proof = document.validate().document;
  const editable = proof.intoDocument();
  editable.setDescription("after proof");
  for (const owner of [document, value, proof, editable]) owner.dispose();
  assert.equal(liveStorageOwners(), baseline);
});

// DOC-1: exact owner transfer is invisible on native batch refusal and aliases.
test("exact owners survive failed batches, repeated references and whole-part setters", () => {
  const baseline = liveStorageOwners(),
    document = sdk.Document.create(),
    value = exact("9007199254740993"),
    alias = value;
  const before = document.toBytes();
  assert.throws(
    () =>
      document.edit([
        { op: "set", pointer: "/x-exact", value: { a: value, b: alias } },
        { op: "remove", pointer: "/missing" },
      ]),
    (e) => e instanceof sdk.EditError && e.code === "missing-value",
  );
  assert.deepEqual(document.toBytes(), before);
  assert.equal(alias.disposed, false);
  assert.equal(value.text, "9007199254740993");
  const read = bridge.WasmJson.prototype.text;
  bridge.WasmJson.prototype.text = () => {
    throw new Error("unexpected exact-value read across the bridge");
  };
  try {
    document.edit([
      { op: "set", pointer: "/x-exact", value: { a: value, b: alias } },
    ]);
    document.setOperation("run", {
      input: { const: value },
      output: { const: alias },
    });
    document.setOperations({ run: { input: { const: value } } });
    document.setExample("run", "sample", { input: value });
  } finally {
    bridge.WasmJson.prototype.text = read;
  }
  assert.equal(alias.text, "9007199254740993");
  const checked = document.validate();
  assert.equal(checked.status, "conformant");
  checked.document.dispose();
  value.dispose();
  document.dispose();
  assert.equal(liveStorageOwners(), baseline);
});

// DOC-1: exact input byte preflight includes repeated occurrences before joining.
test("an oversized exact batch returns borrowed owners without changing bytes", () => {
  const baseline = liveStorageOwners(),
    document = sdk.Document.create();
  const value = exact('"' + "x".repeat(34 * 1024 * 1024) + '"');
  const before = document.toBytes();
  assert.throws(
    () =>
      document.edit([
        { op: "set", pointer: "/x-large", value: [value, value] },
      ]),
    (e) => e instanceof sdk.EditError && e.code === "byte-limit",
  );
  assert.deepEqual(document.toBytes(), before);
  assert.equal(value.disposed, false);
  value.dispose();
  document.dispose();
  assert.equal(liveStorageOwners(), baseline);
});
