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
  checked.document.dispose();
  const contract = operation.input();
  operation.dispose();
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
          originalTake = prototype[take],
          free = prototype.free;
        let freed = 0;
        const thrown = new Error("injected bridge boundary");
        prototype.free = function () {
          freed++;
          return free.call(this);
        };
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
          assert.equal(freed, 1, fault + ": wrapper freed once");
          assert.equal(
            testing.liveStorageOwners(),
            baseline,
            fault + ": no leaked child storage",
          );
        } finally {
          prototype.result = result;
          prototype[take] = originalTake;
          prototype.free = free;
        }
      }
    }
    const proof = document.validate().document,
      selected = proof.operation("run");
    assert.equal(selected.key, "run");
    selected.dispose();
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

test("canonical collections release adopted and untaken owners on partial transfer failure", () => {
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
  for (const [type, method, onOperation] of [
    [sdk.Operation, "operations", false],
    [sdk.Source, "sources", false],
    [sdk.Binding, "bindings", false],
    [sdk.Dependency, "dependencies", false],
    [sdk.Binding, "bindings", true],
    [sdk.Example, "examples", true],
  ])
    for (const failAt of [1, 2, 3]) {
      const baseline = testing.liveStorageOwners();
      const draft = sdk.Document.parse(text).value,
        proof = draft.validate().document;
      const parent = onOperation ? proof.operation("a") : proof;
      const original = type.fromRaw,
        adopted = [],
        rawOwners = [];
      const thrown = new Error("injected collection adoption failure");
      let attempts = 0;
      type.fromRaw = (raw) => {
        rawOwners.push(raw);
        if (++attempts === failAt) throw thrown;
        const owner = original(raw);
        adopted.push(owner);
        return owner;
      };
      try {
        assert.throws(
          () => parent[method](),
          (error) => error === thrown,
        );
        assert.equal(attempts, failAt);
        assert.equal(adopted.length, failAt - 1);
        assert.ok(adopted.every((owner) => owner.disposed));
        assert.ok(rawOwners.every((owner) => owner.__wbg_ptr === 0));
      } finally {
        type.fromRaw = original;
        parent.dispose();
        proof.dispose();
        draft.dispose();
      }
      assert.equal(
        testing.liveStorageOwners(),
        baseline,
        `${type.name}.${method}: no unadopted retained document`,
      );
    }
});
