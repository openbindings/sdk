import test from "node:test";
import assert from "node:assert/strict";

let registrations = 0;
const registered = new WeakSet();
const Registry = globalThis.FinalizationRegistry;
globalThis.FinalizationRegistry = class extends Registry {
  register(...args) {
    registrations++;
    registered.add(args[0]);
    return super.register(...args);
  }
};
const sdk = await import("../dist/index.js");
globalThis.FinalizationRegistry = Registry;
const testing = await import("../dist/testing.js");
await sdk.initialize();

test("owner acquisition is a method; property access creates no owner", () => {
  const paths = [
    [sdk.ExactMember, ["name", "value"]],
    [sdk.ParsedDocument, ["value", "schemas"]],
    [sdk.ValidatedDocument, ["value", "schemas"]],
    [sdk.OperationView, ["value", "input", "output"]],
    [sdk.DependencyView, ["value"]],
    [sdk.BindingView, ["value", "content"]],
    [sdk.SourceView, ["value", "content"]],
    [sdk.ExampleView, ["value", "input", "output"]],
  ];
  const before = registrations;
  for (const [type, names] of paths) {
    for (const name of names) {
      assert.equal(
        typeof type.prototype[name],
        "function",
        `${type.name}.${name}`,
      );
      let prototype = type.prototype;
      while (!Object.hasOwn(prototype, name))
        prototype = Object.getPrototypeOf(prototype);
      assert.equal(
        Object.getOwnPropertyDescriptor(prototype, name).get,
        undefined,
      );
    }
  }
  assert.equal(
    registrations,
    before,
    "reading every acquisition property allocates no handle",
  );
  const baseline = testing.liveStorageOwners();
  const document = sdk.parseDocument(
    '{"openbindings":"0.2.0","operations":{}}',
  ).value;
  const count = registrations;
  assert.equal(document.value, sdk.ParsedDocument.prototype.value);
  assert.equal(
    registrations,
    count,
    "instance property access allocates no handle",
  );
  const first = document.value();
  const second = document.value();
  assert.notEqual(first, second);
  assert(
    registered.has(first) && registered.has(second),
    "each call registers a fresh disposable owner",
  );
  document.dispose();
  first.dispose();
  assert.equal(second.text, '{"openbindings":"0.2.0","operations":{}}');
  second.dispose();
  assert.equal(testing.liveStorageOwners(), baseline);
  for (const [type, name] of [
    [sdk.ExactJson, "text"],
    [sdk.ParsedDocument, "operations"],
    [sdk.OwnedDocumentDraft, "value"],
  ]) {
    assert.equal(
      typeof Object.getOwnPropertyDescriptor(type.prototype, name).get,
      "function",
    );
  }
});
