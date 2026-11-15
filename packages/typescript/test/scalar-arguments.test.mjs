import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
await sdk.initialize();

test("wrong scalar argument types never reach Wasm string marshalling", () => {
  const d = sdk.Document.parse(
    '{"openbindings":"0.2.0","schemas":{"s":true},"operations":{"run":{"input":{"type":"integer"},"examples":{"e":{"input":1}}}},"sources":{"s":{"kind":"echo"}},"bindings":{"b":{"operation":"run","source":"s"}},"dependencies":{"d":{"operation":"run"}}}',
  ).value;
  const proof = d.validate().document,
    operation = proof.operation("run"),
    schema = proof.inputSchema(operation.key);
  const prepared = schema.prepare().schema,
    value = sdk.parseJson('{"a":1}').value;
  const resources = new sdk.ExternalSchemas();
  const uses = [
    (x) => proof.operation(x),
    (x) => proof.resolveOperation(x),
    (x) => proof.source(x),
    (x) => proof.binding(x),
    (x) => proof.dependency(x),
    (x) => proof.schema(x),
    ...[
      "inputSchema",
      "outputSchema",
      "operationBindings",
      "examples",
      "operationValue",
      "bindingValue",
      "sourceValue",
      "dependencyValue",
      "sourceData",
      "bindingData",
    ].map((method) => (x) => proof[method](x)),
    ...["example", "exampleValue", "exampleInput", "exampleOutput"].flatMap(
      (method) => [
        (x) => proof[method](x, "e"),
        (x) => proof[method]("run", x),
      ],
    ),
    (x) => value.get(x),
    (x) => value.at(x),
    (x) => d.remove(x),
    (x) => sdk.checkVersion(x),
    (x) => resources.prepare(x),
    (x) => resources.with(x, value),
    (x) => new sdk.ExternalSchemas([[x, value]]),
  ];
  try {
    for (const wrong of [
      7,
      null,
      undefined,
      false,
      {},
      [],
      Symbol("private"),
      new String("run"),
    ]) {
      for (const use of uses) {
        assert.throws(
          () => use(wrong),
          (error) =>
            error instanceof TypeError &&
            !(error instanceof WebAssembly.RuntimeError),
        );
        assert.equal(prepared.evaluate(7).status, "satisfies");
      }
    }
    const healthy = proof.operation("run");
    assert.equal(healthy.key, "run");
  } finally {
    for (const owner of [resources, value, prepared, schema, proof, d])
      owner.dispose();
  }
});
