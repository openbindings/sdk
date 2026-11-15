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
    [sdk.Document, ["value"]],
    [
      sdk.ConformantDocument,
      [
        "value",
        "schemas",
        "schema",
        "inputSchema",
        "outputSchema",
        "operationValue",
        "bindingValue",
        "sourceValue",
        "dependencyValue",
        "exampleValue",
        "sourceData",
        "bindingData",
        "exampleInput",
        "exampleOutput",
      ],
    ],
    [sdk.Schema, ["json", "prepare"]],
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
  const document = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{}}',
  ).value;
  const count = registrations;
  assert.equal(document.value, sdk.Document.prototype.value);
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
  for (const [type, name] of [[sdk.ExactJson, "text"]]) {
    let prototype = type.prototype;
    while (!Object.hasOwn(prototype, name))
      prototype = Object.getPrototypeOf(prototype);
    assert.equal(
      typeof Object.getOwnPropertyDescriptor(prototype, name).get,
      "function",
    );
  }
  const draft = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{"run":{"tags":["tag"]}}}',
  ).value;
  const proof = draft.validate().document;
  const beforeRecords = registrations;
  const firstRecord = proof.operation("run"),
    secondRecord = proof.operation("run");
  const listed = proof.operations();
  assert.equal(
    registrations,
    beforeRecords,
    "canonical reads allocate no registered owners",
  );
  assert.notEqual(firstRecord, secondRecord);
  assert.ok(!registered.has(firstRecord));
  for (const record of [firstRecord, secondRecord, ...listed]) {
    assert.ok(Object.isFrozen(record));
    assert.equal(Object.getOwnPropertyDescriptor(record, "key").get, undefined);
    assert.equal(record.dispose, undefined);
    assert.equal(record.retain, undefined);
    assert.equal(record.document, undefined);
  }
  proof.dispose();
  draft.dispose();
  assert.equal(firstRecord.key, "run");
  assert.equal(testing.liveStorageOwners(), baseline);
});
