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
  const document = sdk.parseDocument(
    '{"openbindings":"0.2.0","operations":{"run":{"input":{"type":"integer"}}}}',
  ).value;
  const context = document.contracts({ cacheCapacity: 0 });
  try {
    for (const [prototype, invoke, take] of [
      [
        bridge.WasmOperationSelection.prototype,
        () => document.resolveOperation("run"),
        "takeOperation",
      ],
      [
        bridge.WasmPreparation.prototype,
        () => context.prepare("run", "input"),
        "takeContract",
      ],
    ]) {
      for (const fault of [
        "invalid-json",
        "unknown-state",
        "throw-result",
        "throw-take",
        "missing-take",
      ]) {
        const baseline = sdk.liveStorageOwners();
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
            sdk.liveStorageOwners(),
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
    const prepared = context.prepare("run", "input");
    assert.equal(prepared.status, "ready");
    assert.equal(prepared.contract.validate(7).outcome, "satisfies");
    prepared.contract.dispose();
  } finally {
    context.dispose();
    document.dispose();
  }
});
