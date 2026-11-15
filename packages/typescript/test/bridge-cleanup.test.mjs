import * as testing from "../dist/testing.js";
import { prepareInput } from "./conformant-contracts.mjs";
// Internal fault injection complements the installed public-consumer checks.
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

test("owned preparation wrappers release untaken children on decode/take failure", () => {
  const document = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{"run":{"input":{"type":"integer"}}}}',
  ).value;
  const checked = document.validate();
  assert.equal(checked.status, "conformant");
  const operation = checked.document.resolveOperation("run");
  const contract = checked.document.inputSchema(operation.key);
  checked.document.dispose();
  try {
    // The first preparation initializes instance-lifetime evaluator arenas;
    // warm them so every fault compares against a settled baseline.
    const warm = contract.prepare();
    assert.equal(warm.status, "ready");
    warm.schema.dispose();
    for (const [prototype, invoke, take] of [
      [
        bridge.WasmPreparation.prototype,
        () => contract.prepare(),
        "takeSchema",
      ],
    ]) {
      for (const fault of [
        "invalid-json",
        "unknown-state",
        "throw-result",
        "throw-take",
        "missing-take",
      ]) {
        const baseline = testing.liveStorageOwners();
        const result = prototype.result,
          originalTake = prototype[take];
        const thrown = new Error("injected bridge boundary");
        if (fault === "invalid-json") prototype.result = () => "{";
        if (fault === "unknown-state")
          prototype.result = () => '{"status":"unexpected"}';
        if (fault === "throw-result")
          prototype.result = () => {
            throw thrown;
          };
        if (fault === "throw-take")
          prototype[take] = () => {
            throw thrown;
          };
        if (fault === "missing-take") prototype[take] = () => undefined;
        try {
          assert.throws(invoke);
          assert.equal(
            testing.liveStorageOwners(),
            baseline,
            fault + ": no leaked child storage",
          );
        } finally {
          prototype.result = result;
          prototype[take] = originalTake;
        }
      }
    }
    const proof = document.validate().document,
      selected = proof.operation("run");
    assert.equal(selected.key, "run");
    assert.ok(Object.isFrozen(selected));
    proof.dispose();
    const prepared = contract.prepare();
    assert.equal(prepared.status, "ready");
    assert.equal(prepared.schema.evaluate(7).status, "satisfies");
    prepared.schema.dispose();
  } finally {
    contract.dispose();
    document.dispose();
  }
});

test("ordinary admission maps only known engine limits and preserves unexpected exceptions", () => {
  const document = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{"run":{"input":true}}}',
  ).value;
  const setup = prepareInput(document, "run");
  assert.equal(setup.status, "ready");
  const prepared = setup.schema;
  const parse = bridge.WasmJson.parseText;
  try {
    const before = testing.liveStorageOwners();
    bridge.WasmJson.parseText = () => {
      throw '{"kind":"Limit","message":"fixture admission limit"}';
    };
    assert.deepEqual(prepared.evaluate(7), {
      status: "input-error",
      error: {
        code: "input-limit",
        instancePointer: null,
        message:
          "The ordinary value exceeds the admitted JSON limits; parse it with parseJson and larger limits.",
      },
    });
    const unexpected = new Error("unexpected engine error");
    bridge.WasmJson.parseText = () => {
      throw unexpected;
    };
    assert.throws(
      () => prepared.evaluate(7),
      (error) => error === unexpected,
    );
    assert.equal(testing.liveStorageOwners(), before);
  } finally {
    bridge.WasmJson.parseText = parse;
    prepared.dispose();
    document.dispose();
  }
});

test("canonical collection decode failures retain no document owners", () => {
  const text = JSON.stringify({
    openbindings: "0.2.0",
    operations: {
      a: { examples: { a: {}, b: {}, c: {} } },
      b: {},
      c: {},
    },
    sources: { a: { kind: "k" }, b: { kind: "k" }, c: { kind: "k" } },
    bindings: Object.fromEntries(
      ["a", "b", "c"].map((key) => [key, { operation: "a", source: key }]),
    ),
    dependencies: Object.fromEntries(
      ["a", "b", "c"].map((key) => [key, { operation: key }]),
    ),
  });
  const warm = sdk.Document.parse(text).value,
    warmProof = warm.validate().document;
  warmProof.dispose();
  warm.dispose();
  for (const [method, args] of [
    ["operations", []],
    ["sources", []],
    ["bindings", []],
    ["dependencies", []],
    ["operationBindings", ["a"]],
    ["examples", ["a"]],
  ]) {
    for (const failAt of [1, 2, 3]) {
      const baseline = testing.liveStorageOwners();
      const draft = sdk.Document.parse(text).value,
        proof = draft.validate().document;
      const freeze = Object.freeze;
      const thrown = new Error("injected collection decode failure");
      const decoded = [];
      let attempts = 0;
      // Invoke the real bridge, then fail at each record freeze. Rust entries
      // have already dropped when the text crosses to JS, so no partial decode
      // can leave an untaken document owner.
      Object.freeze = (value) => {
        if (++attempts === failAt) throw thrown;
        decoded.push(value);
        return freeze(value);
      };
      try {
        assert.throws(
          () => proof[method](...args),
          (error) => error === thrown,
        );
        assert.ok(decoded.every(Object.isFrozen));
      } finally {
        Object.freeze = freeze;
        proof.dispose();
        draft.dispose();
      }
      assert.equal(testing.liveStorageOwners(), baseline, method);
    }
  }
});
