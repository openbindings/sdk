import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
await sdk.initialize();
function fixture() {
  const doc = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{"run":{"input":{"type":"integer"}}}}',
  ).value;
  const proof = doc.validate().document,
    operation = proof.operation("run"),
    schema = operation.input(),
    prepared = schema.prepare().schema;
  const exact = sdk.parseJson("7").value,
    resources = new sdk.ExternalSchemas([["urn:input", exact]]);
  const sentinel = prepared.retain();
  return {
    doc,
    proof,
    operation,
    schema,
    prepared,
    exact,
    resources,
    sentinel,
    dispose() {
      for (const owner of [
        sentinel,
        resources,
        exact,
        prepared,
        schema,
        operation,
        proof,
        doc,
      ])
        owner.dispose();
    },
  };
}
const lifecycle = (error) =>
  error instanceof sdk.SdkError && error.code === "disposed-handle";
function reenter(dispose) {
  return new Proxy(
    {},
    {
      ownKeys() {
        dispose();
        return [];
      },
    },
  );
}

test("callback disposal is checked after options and ordinary conversion", () => {
  const cases = [
    (f) =>
      f.schema.prepare({
        get limits() {
          f.schema.dispose();
          return {};
        },
      }),
    (f) =>
      f.resources.prepare("urn:input", {
        get limits() {
          f.resources.dispose();
          return {};
        },
      }),
    (f) =>
      f.prepared.explain(
        new Proxy(
          { n: 1 },
          {
            ownKeys(target) {
              f.prepared.dispose();
              return Reflect.ownKeys(target);
            },
          },
        ),
      ),
    (f) =>
      f.prepared.evaluate(
        7,
        reenter(() => f.prepared.dispose()),
      ),
    (f) =>
      f.prepared.evaluateJson(
        "7",
        reenter(() => f.prepared.dispose()),
      ),
    (f) =>
      f.prepared.evaluate(
        f.exact,
        reenter(() => f.exact.dispose()),
      ),
    (f) =>
      f.resources.locate({
        sourceUri: null,
        get pointer() {
          f.resources.dispose();
          return "";
        },
      }),
    (f) =>
      f.proof.locate({
        sourceUri: null,
        get pointer() {
          f.proof.dispose();
          return "";
        },
      }),
  ];
  for (const run of cases) {
    const f = fixture();
    try {
      assert.throws(() => run(f), lifecycle);
      assert.equal(f.sentinel.evaluate(7).status, "satisfies");
    } finally {
      f.dispose();
    }
  }
});
