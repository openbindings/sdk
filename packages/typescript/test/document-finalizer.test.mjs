// Deterministic lifecycle-path probe. The registry shim stores weak target/token
// references and lets the test deliver a chosen finalizer callback without GC.
// node:test isolates this file in its own process. This deliberately controls
// callback delivery, not real GC eligibility or scheduling.
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const registrations = [];
class ControlledFinalizationRegistry {
  constructor(callback) {
    this.callback = callback;
  }
  register(target, held, token) {
    registrations.push({
      registry: this,
      target: new WeakRef(target),
      held,
      token: token === undefined ? undefined : new WeakRef(token),
      active: true,
    });
  }
  unregister(token) {
    let found = false;
    for (const record of registrations) {
      if (
        record.active &&
        record.registry === this &&
        record.token?.deref() === token
      ) {
        record.active = false;
        record.held = undefined;
        found = true;
      }
    }
    return found;
  }
}
globalThis.FinalizationRegistry = ControlledFinalizationRegistry;
const sdk = await import("../dist/index.js");
const testing = await import("../dist/testing.js");
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const registrationFor = (target) =>
  registrations.find((row) => row.active && row.target.deref() === target);
function deliver(record) {
  if (!record?.active) return false;
  const held = record.held;
  record.held = undefined;
  record.active = false;
  record.registry.callback(held);
  return true;
}
test("finalizing an editable document preserves separately acquired exact owners", () => {
  // Independently acquired leaves keep their own finalizers.
  const baseline = testing.liveStorageOwners();
  let document = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{"run":{"input":9007199254740993}},"schemas":{"removed":false}}',
  ).value;
  const root = document.value();
  const leaf = root.at("/operations/run/input"),
    removed = root.at("/schemas/removed");
  root.dispose();
  document = replaceDocument(document, document.remove("/schemas/removed"));
  const documentRecord = registrationFor(document),
    removedRecord = registrationFor(removed);
  assert.ok(documentRecord);
  assert.ok(removedRecord);
  deliver(documentRecord);
  assert.equal(leaf.disposed, false);
  assert.equal(leaf.text, "9007199254740993");
  let rebuilt = sdk.Document.create();
  rebuilt = replaceDocument(
    rebuilt,
    rebuilt.set("/operations/" + "run", { input: leaf }),
  );
  rebuilt.dispose();
  deliver(removedRecord);
  leaf.dispose();
  assert.equal(testing.liveStorageOwners(), baseline);
});

// Temporary exact transfer reattaches a finalizer even after refusal.
test("a returned exact owner is finalized once after a failed edit", () => {
  const baseline = testing.liveStorageOwners();
  let document = sdk.Document.create(),
    value = sdk.parseJson("9007199254740993").value;
  const before = registrationFor(value);
  assert.throws(
    () =>
      document.edit([
        { op: "set", pointer: "/x", value },
        { op: "remove", pointer: "/absent" },
      ]),
    (e) => e.code === "missing-value",
  );
  assert.equal(before.active, false);
  assert.equal(value.text, "9007199254740993");
  const after = registrationFor(value);
  assert.ok(after);
  assert.equal(deliver(after), true);
  assert.equal(deliver(before), false);
  assert.equal(deliver(after), false);
  document.dispose();
  assert.equal(testing.liveStorageOwners(), baseline);
});

// Test fixture cursor: keep the new immutable snapshot and release its predecessor.
function replaceDocument(previous, next) {
  previous.dispose();
  return next;
}

test("forgotten schema and exact owners release their pinned document when finalized", () => {
  sdk.validateDocument('{"openbindings":"0.2.0","operations":{}}');
  const baseline = testing.liveStorageOwners();
  for (const kind of ["schema", "exact"]) {
    const document = sdk.Document.parse(
      JSON.stringify({
        openbindings: "0.2.0",
        operations: { run: { input: true } },
        "x-unrelated": "x".repeat(1024 * 1024),
      }),
    ).value;
    const proof = document.validate().document;
    const record = proof.operation("run");
    const held =
      kind === "schema"
        ? proof.inputSchema(record.key)
        : proof.operationValue(record.key);
    const finalization = registrationFor(held);
    assert.ok(finalization);
    assert.equal(registrationFor(record), undefined);
    assert.ok(document.retainedBytes() > 1024 * 1024);
    proof.dispose();
    document.dispose();
    assert.equal(testing.liveStorageOwners(), baseline + 1);
    // Deliver the registered callback deterministically; this does not claim
    // that a host will schedule GC or finalizers before the next request.
    assert.equal(deliver(finalization), true);
    assert.equal(deliver(finalization), false);
    assert.equal(testing.liveStorageOwners(), baseline);
    assert.equal(record.key, "run");
  }
});
