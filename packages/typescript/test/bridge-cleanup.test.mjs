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

test("owned selection and preparation wrappers release untaken children on decode/take failure", () => {
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
    warm.valueContract.dispose();
    for (const [prototype, invoke, take] of [
      [
        bridge.WasmOperationSelection.prototype,
        () => document.resolveOperation("run"),
        "takeOperation",
      ],
      [
        bridge.WasmPreparation.prototype,
        () => contract.prepare(),
        "takeValueContract",
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
    const selected = document.resolveOperation("run");
    assert.equal(selected.status, "found");
    selected.operation.dispose();
    const prepared = contract.prepare();
    assert.equal(prepared.status, "ready");
    assert.equal(prepared.valueContract.validate(7).outcome, "satisfies");
    prepared.valueContract.dispose();
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
  const prepared = setup.valueContract;
  const parse = bridge.WasmJson.parseText;
  try {
    const before = testing.liveStorageOwners();
    bridge.WasmJson.parseText = () => {
      throw '{"kind":"Limit","message":"fixture admission limit"}';
    };
    assert.deepEqual(prepared.validate(7), {
      outcome: "input-error",
      error: {
        code: "input-limit",
        instancePointer: null,
        message: "The ordinary value exceeds the admitted JSON limits.",
      },
    });
    const unexpected = new Error("unexpected engine error");
    bridge.WasmJson.parseText = () => {
      throw unexpected;
    };
    assert.throws(
      () => prepared.validate(7),
      (error) => error === unexpected,
    );
    assert.equal(testing.liveStorageOwners(), before);
  } finally {
    bridge.WasmJson.parseText = parse;
    prepared.dispose();
    document.dispose();
  }
});
