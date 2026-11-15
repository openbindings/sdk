// Public witnesses for eager detached records, including the compact text transport.
import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
import { liveStorageOwners } from "../dist/testing.js";

// The kind helper is ordinary data logic, usable before initialization.
assert.equal(sdk.acceptsKind({ key: "d", operationKey: "op" }, "\ud800"), true);
assert.equal(
  sdk.acceptsKind({ key: "d", operationKey: "op", kinds: ["kind"] }, "other"),
  false,
);
assert.throws(
  () => sdk.acceptsKind({ key: "d", operationKey: "op" }, 7),
  TypeError,
);
await sdk.initialize();

test("operation records preserve presence, exact text, eager fields and zero retained storage", () => {
  sdk.validateDocument('{"openbindings":"0.2.0","operations":{}}');
  const baseline = liveStorageOwners();
  let cases = 0;
  for (const key of ["__proto__", "constructor"]) {
    for (const description of [
      undefined,
      "",
      'description\u0000𝄞"\\',
      "long" + "x💡".repeat(600),
    ]) {
      for (const deprecated of [undefined, false, true]) {
        for (const aliases of [undefined, [], ["alias", "name2"]]) {
          for (const tags of [
            undefined,
            [],
            ["second", "first", "tag💡"],
            Array.from({ length: 20 }, (_, i) => `tag${i}💡`),
          ]) {
            const entry = {
              description,
              deprecated,
              aliases,
              tags,
              input: false,
              output: true,
            };
            const parsed = sdk.Document.parse(
              JSON.stringify({
                openbindings: "0.2.0",
                operations: Object.fromEntries([[key, entry]]),
              }),
            );
            assert.equal(parsed.status, "parsed");
            const checked = parsed.value.validate();
            assert.equal(checked.status, "conformant");
            const proof = checked.document;
            const record = proof.operation(key);
            assert.notEqual(
              record,
              proof.operation(key),
              "each call constructs its record",
            );
            const expected = Object.fromEntries(
              Object.entries({
                key,
                description,
                deprecated,
                aliases,
                tags,
              }).filter(([, value]) => value !== undefined),
            );
            assert.deepEqual(record, expected);
            assert.deepEqual(
              Reflect.ownKeys(record).sort(),
              Object.keys(expected).sort(),
            );
            for (const descriptor of Object.values(
              Object.getOwnPropertyDescriptors(record),
            ))
              assert.equal(
                descriptor.get,
                undefined,
                "fields are eager data properties",
              );
            assert.deepEqual(record, proof.operations()[0]);
            if (aliases?.length) {
              assert.deepEqual(record, proof.resolveOperation(aliases[0]));
              assert.equal(
                proof.operation(aliases[0]),
                undefined,
                "primary lookup excludes aliases",
              );
            }
            assert.equal(proof.operation("missing"), undefined);
            assert.equal(proof.operation("key\u0000💡"), undefined);
            assert.equal(Object.getPrototypeOf(record), Object.prototype);
            assert.ok(Object.isFrozen(record));
            assert.throws(() => {
              record.key = "changed";
            }, TypeError);
            for (const value of Object.values(record)) {
              if (Array.isArray(value)) {
                assert.ok(Object.isFrozen(value));
                assert.throws(() => value.push("changed"), TypeError);
              } else assert.ok(["string", "boolean"].includes(typeof value));
            }
            for (const method of [
              "dispose",
              "retain",
              "document",
              "input",
              "value",
            ])
              assert.equal(Object.hasOwn(record, method), false);
            proof.dispose();
            parsed.value.dispose();
            assert.deepEqual(record, expected, "metadata outlives both roots");
            assert.equal(
              liveStorageOwners(),
              baseline,
              "metadata retains no Wasm storage",
            );
            cases++;
          }
        }
      }
    }
  }
  assert.equal(cases, 288);
});

test("every canonical read returns only declared frozen data and proof queries retain exact payloads", () => {
  sdk.validateDocument('{"openbindings":"0.2.0","operations":{}}');
  const baseline = liveStorageOwners();
  const draft = sdk.Document.parse(String.raw`{
    "openbindings":"0.2.0",
    "operations":{"run":{"aliases":["alias"],"input":false,"output":true,"examples":{"e":{"description":"example","input":null,"output":9007199254740993,"x-exact":1.00}},"x-exact":1.00}},
    "sources":{"s":{"kind":"kind","data":null,"x-exact":1.00}},
    "bindings":{"b":{"operation":"run","source":"s","preference":0,"idempotent":false,"deprecated":false,"data":null,"x-exact":1.00}},
    "dependencies":{"d":{"operation":"run","kinds":["kind"],"x-exact":1.00}}
  }`).value;
  const proof = draft.validate().document;
  const retained = [];
  for (const [read, expected] of [
    [() => proof.operation("run"), { key: "run", aliases: ["alias"] }],
    [() => proof.resolveOperation("alias"), { key: "run", aliases: ["alias"] }],
    [() => proof.operations()[0], { key: "run", aliases: ["alias"] }],
    ...[() => proof.source("s"), () => proof.sources()[0]].map((read) => [
      read,
      { key: "s", kind: "kind" },
    ]),
    ...[
      () => proof.binding("b"),
      () => proof.bindings()[0],
      () => proof.operationBindings("run")[0],
    ].map((read) => [
      read,
      {
        key: "b",
        operationKey: "run",
        sourceKey: "s",
        preference: 0,
        idempotent: false,
        deprecated: false,
      },
    ]),
    ...[() => proof.dependency("d"), () => proof.dependencies()[0]].map(
      (read) => [read, { key: "d", operationKey: "run", kinds: ["kind"] }],
    ),
    ...[() => proof.example("run", "e"), () => proof.examples("run")[0]].map(
      (read) => [read, { key: "e", description: "example" }],
    ),
  ]) {
    const record = read();
    retained.push([record, expected]);
    assert.deepEqual(record, expected);
    assert.notEqual(record, read());
    assert.deepEqual(
      Reflect.ownKeys(record).sort(),
      Object.keys(expected).sort(),
    );
    assert.equal(Object.getPrototypeOf(record), Object.prototype);
    assert.ok(Object.isFrozen(record));
    for (const descriptor of Object.values(
      Object.getOwnPropertyDescriptors(record),
    )) {
      assert.equal(descriptor.get, undefined);
      assert.equal(descriptor.writable, false);
      if (Array.isArray(descriptor.value))
        assert.ok(Object.isFrozen(descriptor.value));
    }
  }
  for (const [method, args] of [
    ["operationValue", ["run"]],
    ["bindingValue", ["b"]],
    ["sourceValue", ["s"]],
    ["dependencyValue", ["d"]],
    ["exampleValue", ["run", "e"]],
  ]) {
    const value = proof[method](...args),
      extension = value.get("x-exact");
    assert.equal(extension.text, "1.00");
    extension.dispose();
    value.dispose();
  }
  for (const [method, args, text] of [
    ["sourceData", ["s"], "null"],
    ["bindingData", ["b"], "null"],
    ["exampleInput", ["run", "e"], "null"],
    ["exampleOutput", ["run", "e"], "9007199254740993"],
  ]) {
    const value = proof[method](...args);
    assert.equal(value.text, text);
    value.dispose();
  }
  const queries = [
    ...[
      "operationValue",
      "inputSchema",
      "outputSchema",
      "operationBindings",
      "examples",
    ].map((m) => [m, ["alias"]]),
    ...[
      "bindingValue",
      "sourceValue",
      "dependencyValue",
      "sourceData",
      "bindingData",
    ].map((m) => [m, ["missing"]]),
    ...["example", "exampleValue", "exampleInput", "exampleOutput"].flatMap(
      (m) => [
        [m, ["alias", "e"]],
        [m, ["run", "missing"]],
      ],
    ),
  ];
  for (const [method, args] of queries)
    assert.equal(proof[method](...args), undefined, method);
  for (const direction of ["input", "output"]) {
    const schema = proof[`${direction}Schema`]("run"),
      value = schema.json();
    assert.equal(value.text, direction === "input" ? "false" : "true");
    value.dispose();
    schema.dispose();
  }
  proof.dispose();
  draft.dispose();
  for (const [record, expected] of retained) assert.deepEqual(record, expected);
  assert.equal(liveStorageOwners(), baseline);
  assert.equal(
    sdk.acceptsKind(retained.find(([r]) => r.kinds)[0], "kind"),
    true,
  );
  for (const [method, args] of queries)
    assert.throws(
      () => proof[method](...args),
      (error) => error.code === "disposed-handle",
      method,
    );
});
