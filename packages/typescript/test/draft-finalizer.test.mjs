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
test("abandoning an editing scope cannot finalize reachable exact leaves", () => {
  const baseline = sdk.liveStorageOwners();
  const escaped = (() => {
    const parsed = sdk.parseDocument(
      '{"openbindings":"0.2.0","operations":{"run":{"input":9007199254740993}},"schemas":{"removed":false}}',
    );
    assert.equal(parsed.status, "parsed");
    const converted = parsed.value.toDraft();
    assert.equal(converted.status, "drafted");
    const scope = converted.draft,
      root = scope.value;
    const scopeRecord = registrationFor(scope);
    const removedRecord = registrationFor(root.schemas.removed);
    assert.ok(
      removedRecord,
      "converted leaves retain their ordinary finalizer fallback",
    );
    delete root.schemas.removed;
    parsed.value.dispose();
    return { subtree: root.operations.run, scopeRecord, removedRecord };
  })();
  // The caller retains a subtree but has abandoned its scope and root. Old code has
  // an aggregate finalizer to deliver; corrected code must have no such callback.
  const scopeCallbackDelivered = deliver(escaped.scopeRecord);
  const reachableLeafDisposed = escaped.subtree.input.disposed;
  let rebuilding;
  try {
    const built = sdk.authorDocument({ operations: { run: escaped.subtree } });
    rebuilding = built.status;
    if (built.status === "authored") built.document.dispose();
  } catch (error) {
    rebuilding = error.code ?? String(error);
  }
  // Simulate eventual collection of a removed leaf, then explicitly release the
  // still-reachable leaf. A fresh arena must be releasable in either implementation.
  deliver(escaped.removedRecord);
  escaped.subtree.input.dispose();
  const arenasReleased = sdk.liveStorageOwners() === baseline;
  const output = {
    scopeCallbackDelivered,
    reachableLeafDisposed,
    rebuilding,
    arenasReleased,
    expectedPolicyPass:
      !scopeCallbackDelivered &&
      !reachableLeafDisposed &&
      rebuilding === "authored" &&
      arenasReleased,
    limitation:
      "Controlled finalizer delivery tests the disposal path; it is not a claim about real GC scheduling.",
  };
  console.log(JSON.stringify(output, null, 2));
  assert.equal(
    output.expectedPolicyPass,
    true,
    "GC abandonment must not revoke reachable draft leaves",
  );
});
