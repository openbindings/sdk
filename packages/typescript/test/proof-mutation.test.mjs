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

test("raw JSON and wrong canonical owners cannot be borrowed to produce schemas", () => {
  const document = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{"run":{"input":true,"examples":{"e":{"input":42}}}}}',
  ).value;
  const proof = document.validate().document,
    operation = proof.operation("run"),
    example = operation.example("e");
  const raw = document.value(),
    input = raw.at("/operations/run/input");
  const refused = (error) =>
    error instanceof sdk.SdkError && error.code === "document-conformance";
  for (const owner of [raw, input, example]) {
    for (const method of ["input", "output"])
      assert.throws(() => sdk.Operation.prototype[method].call(owner), refused);
    for (const method of ["json", "prepare", "retain"])
      assert.throws(() => sdk.Schema.prototype[method].call(owner), refused);
  }
  for (const owner of [input, raw, example, operation, proof, document])
    owner.dispose();
});

test("every canonical Wasm owner checks its Rust variant before borrowed reads", () => {
  const draft = bridge.WasmDocument.parseText(
    '{"openbindings":"0.2.0","operations":{"op":{"input":true,"examples":{"e":{"input":42}}}},"sources":{"s":{"kind":"k"}},"bindings":{"b":{"operation":"op","source":"s"}},"dependencies":{"d":{"operation":"op"}}}',
  );
  const assessment = draft.validate(),
    proof = assessment.takeConformant(),
    operation = proof.operation("op");
  const cases = [
    [
      bridge.WasmOperation,
      operation,
      [
        ["input", []],
        ["output", []],
        ["bindings", []],
        ["examples", []],
        ["example", ["e"]],
      ],
    ],
    [bridge.WasmSource, proof.source("s"), [["data", []]]],
    [
      bridge.WasmBinding,
      proof.binding("b"),
      [
        ["data", []],
        ["operation", []],
        ["source", []],
      ],
    ],
    [
      bridge.WasmDependency,
      proof.dependency("d"),
      [
        ["operation", []],
        ["acceptsKind", ['"k"']],
      ],
    ],
    [
      bridge.WasmExample,
      operation.example("e"),
      [
        ["input", []],
        ["output", []],
      ],
    ],
  ];
  try {
    const refused = (error) =>
      JSON.parse(error).code === "invalid-domain-owner";
    for (const [type, owner, reads] of cases) {
      for (const [otherType, other] of cases) {
        if (type === otherType) continue;
        for (const [method, args] of [
          ["retain", []],
          ["document", []],
          ["value", []],
          ["metadata", []],
          ...reads,
        ])
          assert.throws(
            () => type.prototype[method].call(other, ...args),
            refused,
            `${type.name}.${method} on ${otherType.name}`,
          );
      }
      assert.ok(JSON.parse(owner.metadata()).key);
    }
    const schema = operation.input(),
      json = schema.json();
    assert.equal(json.text(), "true");
    json.free();
    schema.free();
    const value = cases[4][1].input();
    assert.equal(value.text(), "42");
    value.free();
  } finally {
    for (const [, owner] of cases) owner.free();
    proof.free();
    assessment.free();
    draft.free();
  }
});
