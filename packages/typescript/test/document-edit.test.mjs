// Document states, native atomic editing and proof ownership.
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
  let document = open(text);
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
    () => forged.resolveOperation("run"),
    (e) => e.code === "disposed-handle",
  );
  assert.throws(
    () =>
      sdk.ConformantDocument.prototype.resolveOperation.call(document, "run"),
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
  assert.equal(typeof document.resolveOperation, "undefined");
  const root = document.value(),
    exactInput = root.at("/operations/run/input");
  assert.ok(exactInput instanceof sdk.ExactJson);
  assert.equal(exactInput.prepare, undefined);
  exactInput.dispose();
  root.dispose();
  document.dispose();
  proof.dispose();
});

test("consuming proof revokes all aliases while retained proofs and editable copies survive", () => {
  let document = open(text),
    proof = document.validate().document,
    alias = proof,
    retained = proof.retain(),
    independent = proof.asDocument();
  const initial = proof.toBytes();
  independent = replaceDocument(
    independent,
    independent.set("/description", "different revision"),
  );
  assert.deepEqual(proof.toBytes(), initial);
  let editable = proof.intoDocument();
  assert.equal(alias.disposed, true);
  for (const read of [
    () => alias.operations(),
    () => alias.toBytes(),
    () => alias.value(),
    () => alias.resolveOperation("run"),
    () => alias.asDocument(),
    () => alias.intoDocument(),
  ])
    assert.throws(read, (e) => e.code === "disposed-handle");
  editable = replaceDocument(
    editable,
    editable.set("/operations/" + "run" + "/description", "editing again"),
  );
  assert.deepEqual(retained.toBytes(), initial);
  const retainedOperation = retained.operation("run");
  assert.equal(retainedOperation.key, "run");
  retainedOperation.dispose();
  for (const owner of [document, proof, retained, independent, editable])
    owner.dispose();
});

test("one batch commits once and preserves decimal spelling and every untouched byte", () => {
  let document = open(text),
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
    document = replaceDocument(
      document,
      document.edit([
        {
          op: "set",
          pointer: "/operations/run/input",
          value: { const: decimal },
        },
        { op: "set", pointer: "/description", value: "edited" },
      ]),
    );
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

test("failed batches preserve bytes and retained canonical entries", () => {
  let document = open(text);
  const proof = document.validate().document,
    operation = proof.operation("run"),
    before = document.toBytes();
  const oldDescription = operation.description;
  assert.throws(
    () =>
      document.edit([
        { op: "set", pointer: "/name", value: "new" },
        { op: "remove", pointer: "/missing" },
      ]),
    (e) => e instanceof sdk.EditError && e.code === "missing-value",
  );
  assert.deepEqual(document.toBytes(), before);
  assert.equal(operation.description, oldDescription);
  document = replaceDocument(
    document,
    document.set("/operations/run/description", "changed"),
  );
  const current = document.validate().document,
    changed = current.operation("run");
  assert.equal(changed.description, "changed");
  assert.equal(operation.description, oldDescription);
  assert.deepEqual(proof.toBytes(), before);
  for (const owner of [changed, current, operation, proof, document])
    owner.dispose();
});

test("coalesced insertions, escaped keys and explicit namespace edits construct one document", () => {
  let document = sdk.Document.create();
  document = replaceDocument(
    document,
    document.edit(
      Array.from({ length: 1000 }, (_, i) => ({
        op: "set",
        pointer: `/operations/op${i}`,
        value: {},
      })),
    ),
  );
  const populated = document.validate().document,
    entries = populated.operations();
  assert.equal(entries.length, 1000);
  for (const entry of entries) entry.dispose();
  populated.dispose();
  document = replaceDocument(
    document,
    document.edit(
      ["sources", "bindings", "dependencies"].map((name) => ({
        op: "set",
        pointer: "/" + name,
        value: {},
      })),
    ),
  );
  document = replaceDocument(
    document,
    document.set("/operations/" + "run~1~0", { input: true, examples: {} }),
  );
  document = replaceDocument(
    document,
    document.set("/sources/" + "s~1~0", { kind: "custom", data: null }),
  );
  document = replaceDocument(
    document,
    document.set("/bindings/" + "b~1~0", {
      operation: "run/~",
      source: "s/~",
      preference: 0,
    }),
  );
  document = replaceDocument(
    document,
    document.set("/dependencies/" + "d~1~0", { operation: "run/~" }),
  );
  document = replaceDocument(
    document,
    document.set("/operations/" + "run~1~0" + "/examples/" + "e~1~0", {
      input: null,
    }),
  );
  document = replaceDocument(
    document,
    document.set("/operations", { run: { input: true } }),
  );
  document = replaceDocument(
    document,
    document.set("/sources", { s: { kind: "custom", data: null } }),
  );
  document = replaceDocument(
    document,
    document.set("/bindings", { b: { operation: "run", source: "s" } }),
  );
  document = replaceDocument(
    document,
    document.set("/dependencies", { d: { operation: "run" } }),
  );
  document = replaceDocument(
    document,
    document.set("/operations/run/examples", { e: { input: null } }),
  );
  const checked = document.validate();
  assert.equal(checked.status, "conformant");
  checked.document.dispose();
  document.dispose();
});

test("duplicate documents remain inspectable and immutable until replaced by a distinct document", () => {
  let text = '{"openbindings":"0.2.0","operations":{},"x":1,"x":2}',
    document = open(text);
  assert.equal(document.validate().status, "report");
  assert.equal(document.references().limitation.code, "duplicate-members");
  assert.throws(
    () => document.set("/name", "refused"),
    (e) => e.code === "duplicate-members",
  );
  assert.equal(new TextDecoder().decode(document.toBytes()), text);
  document.dispose();
});

test("exact text edits preserve lone UTF-16 and source positions follow the current revision", () => {
  let document = open(text);
  document = replaceDocument(document, document.set("/name", "\ud800"));
  document = replaceDocument(document, document.set("/description", "\udc00"));
  document = replaceDocument(
    document,
    document.set("/operations/" + "run" + "/description", "\ud800"),
  );
  const root = document.value(),
    name = root.get("name"),
    description = root.get("description");
  assert.equal(JSON.parse(name.text), "\ud800");
  assert.equal(JSON.parse(description.text), "\udc00");
  name.dispose();
  description.dispose();
  root.dispose();
  document = replaceDocument(document, document.set("/operations", null));
  const bytes = document.toBytes();
  const value = document.value(),
    operations = value.get("operations");
  const offset = operations.metadata.location.byteOffset;
  assert.equal(
    new TextDecoder().decode(bytes.subarray(offset, offset + 4)),
    "null",
  );
  assert.equal(document.validate().status, "report");
  operations.dispose();
  value.dispose();
  document.dispose();
});

test("released edited owners and consumed proofs release their arenas", () => {
  const warm = open(text);
  warm.validate().document.dispose();
  warm.dispose();
  const baseline = liveStorageOwners();
  let document = open(text),
    value = exact("1e+003");
  document = replaceDocument(document, document.set("/x-number", value));
  const proof = document.validate().document;
  let editable = proof.intoDocument();
  editable = replaceDocument(
    editable,
    editable.set("/description", "after proof"),
  );
  for (const owner of [document, value, proof, editable]) owner.dispose();
  assert.equal(liveStorageOwners(), baseline);
});

// Exact owner transfer is invisible on native batch refusal and aliases.
test("exact owners survive failed batches, repeated references and whole-entry edits", () => {
  let baseline = liveStorageOwners(),
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
    document = replaceDocument(
      document,
      document.edit([
        { op: "set", pointer: "/x-exact", value: { a: value, b: alias } },
      ]),
    );
    document = replaceDocument(
      document,
      document.set("/operations/" + "run", {
        input: { const: value },
        output: { const: alias },
      }),
    );
    document = replaceDocument(
      document,
      document.set("/operations", { run: { input: { const: value } } }),
    );
    document = replaceDocument(
      document,
      document.set("/operations/run/examples", { sample: { input: value } }),
    );
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

// Exact input byte preflight includes repeated occurrences before joining.
test("an oversized exact batch returns borrowed owners without changing bytes", () => {
  let baseline = liveStorageOwners(),
    document = sdk.Document.create();
  const value = exact('"' + "x".repeat(17 * 1024 * 1024) + '"');
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

test("exact namespace construction and exact pointer edits share one atomic bridge call", () => {
  // Every namespace commits once; failed ordinary JSON admission is atomic.
  let document = sdk.Document.create();
  const number = exact("1e+003");
  const rawEdit = bridge.WasmDocument.prototype.editEncoded;
  let calls = 0;
  bridge.WasmDocument.prototype.editEncoded = function (...args) {
    calls++;
    return rawEdit.apply(this, args);
  };
  try {
    document = replaceDocument(
      document,
      document.edit([
        {
          op: "set",
          pointer: "/operations",
          value: {
            run: { input: true, "x-number": number },
          },
        },
        { op: "set", pointer: "/name", value: "Batch" },
        {
          op: "set",
          pointer: "/dependencies",
          value: { d: { operation: "run" } },
        },
        {
          op: "set",
          pointer: "/sources",
          value: { s: { kind: "custom", data: number } },
        },
        {
          op: "set",
          pointer: "/bindings",
          value: { b: { operation: "run", source: "s" } },
        },
      ]),
    );
    assert.equal(calls, 1);
    assert.match(
      new TextDecoder().decode(document.toBytes()),
      /"x-number":1e\+003/,
    );
    assert.equal(number.text, "1e+003");
    const before = document.toBytes();
    assert.throws(
      () =>
        document.edit([
          { op: "set", pointer: "/name", value: "changed" },
          {
            op: "set",
            pointer: "/sources",
            value: {
              s: { kind: NaN },
            },
          },
        ]),
      (e) =>
        e.code === "non-finite-number" && e.inputPointer === "/1/value/s/kind",
    );
    assert.deepEqual(document.toBytes(), before);
    assert.equal(calls, 1);
    assert.throws(() =>
      document.edit([
        { op: "set", pointer: "/operations", value: {} },
        { op: "remove", pointer: "/missing" },
      ]),
    );
    assert.equal(calls, 2);
    assert.deepEqual(document.toBytes(), before);
    document.validate().document.dispose();
  } finally {
    bridge.WasmDocument.prototype.editEncoded = rawEdit;
    number.dispose();
    document.dispose();
  }
});

test("a finding pointer through an unpaired name edits the member it reports", () => {
  const source = String.raw`{"openbindings":"0.2.0","operations":{},"x-a":{"\ud800":1,"k":2}}`;
  const reported = open(source);
  const assessed = reported.validate();
  assert.equal(assessed.status, "report");
  const [finding] = assessed.report.findings;
  assert.equal(finding.rule, "OBI-01");
  const { pointer } = finding.location;
  assert.equal(pointer, "/x-a/\ud800");
  reported.dispose();
  const kept = String.raw`{"openbindings":"0.2.0","operations":{},"x-a":{"k":2}}`;
  const value = exact("[3]");
  for (const [edit, expected] of [
    [(document) => document.remove(pointer), kept],
    [
      (document) => document.set(pointer, 3),
      String.raw`{"openbindings":"0.2.0","operations":{},"x-a":{"\ud800":3,"k":2}}`,
    ],
    [
      (document) => document.set(pointer, value),
      String.raw`{"openbindings":"0.2.0","operations":{},"x-a":{"\ud800":[3],"k":2}}`,
    ],
    [
      (document) =>
        document.edit([
          { op: "remove", pointer },
          { op: "set", pointer: "/x-a/fixed", value: 1 },
        ]),
      String.raw`{"openbindings":"0.2.0","operations":{},"x-a":{"k":2,"fixed":1}}`,
    ],
  ]) {
    let document = open(source);
    document = replaceDocument(document, edit(document));
    assert.equal(new TextDecoder().decode(document.toBytes()), expected);
    const checked = document.validate();
    if (expected === kept) {
      assert.equal(checked.status, "conformant");
      checked.document.dispose();
    }
    document.dispose();
  }
  assert.equal(value.text, "[3]");
  value.dispose();
  let document = open(source);
  assert.throws(
    () => document.remove("\ud800"),
    (e) => e instanceof sdk.EditError && e.code === "invalid-pointer",
  );
  assert.throws(
    () => document.set("/none/\ud800", 0),
    (e) => e instanceof sdk.EditError && e.code === "missing-parent",
  );
  assert.equal(new TextDecoder().decode(document.toBytes()), source);
  document.dispose();
});

// Test fixture cursor: keep the new immutable snapshot and release its predecessor.
function replaceDocument(previous, next) {
  previous.dispose();
  return next;
}

test("edits return distinct owners while the original snapshot and proof remain usable", () => {
  const baseline = liveStorageOwners();
  const original = open(text);
  const proof = original.validate().document;
  const operation = proof.operation("run");
  const originalDescription = operation.description;
  const value = exact("null");
  const revised = original.set("/description", value);
  const removed = revised.remove("/description");
  const empty = original.edit([]);
  try {
    assert.notEqual(revised, original);
    assert.notEqual(empty, original);
    assert.equal(new TextDecoder().decode(original.toBytes()), text);
    assert.deepEqual(proof.toBytes(), original.toBytes());
    assert.equal(operation.description, originalDescription);
    // Release exact reads separately; they do not consume either document.
    const root = revised.value();
    const description = root.get("description");
    assert.equal(description.text, "null");
    description.dispose();
    root.dispose();
    original.dispose();
    assert.equal(revised.disposed, false);
    assert.equal(empty.disposed, false);
    assert.deepEqual(proof.toBytes(), empty.toBytes());
    const removedRoot = removed.value();
    assert.equal(removedRoot.get("description"), undefined);
    removedRoot.dispose();
  } finally {
    for (const owner of [
      original,
      proof,
      operation,
      revised,
      removed,
      empty,
      value,
    ])
      owner.dispose();
  }
  assert.equal(liveStorageOwners(), baseline);
});
