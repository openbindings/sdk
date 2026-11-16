import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
await sdk.initialize();
const fixture = JSON.parse(
  await readFile(
    new URL(
      "../../../crates/openbindings-json-schema-evaluator/tests/fixtures/preparation-limit-parity.json",
      import.meta.url,
    ),
  ),
);

function owners() {
  const document = sdk.Document.parse(fixture.document).value;
  const proof = document.validate().document;
  const operation = proof.operation("run");
  const schema = proof.inputSchema(operation.key);
  const value = sdk.parseJson(fixture.schema).value;
  const resources = new sdk.ExternalSchemas([[fixture.uri, value]]);
  return {
    schema,
    value,
    resources,
    dispose() {
      for (const owner of [resources, value, schema, proof, document])
        owner.dispose();
    },
  };
}

test("preparation defaults come from Rust and work remains uncapped", () => {
  const defaults = sdk.preparationLimits();
  assert.deepEqual(defaults, {
    maxSchemaNodes: 100_000,
    maxIdentityBytes: 8_388_608,
    maxSdkWorkUnits: null,
    maxSdkReservationBytes: 536_870_912,
  });
  assert.equal(Object.isFrozen(defaults), true);
  assert.notEqual(defaults, sdk.preparationLimits());
});

for (const { field, value, code } of fixture.cases) {
  test(`${field} refuses the same input with the same code as Rust, then recovers`, () => {
    const f = owners();
    try {
      const preparationLimits = { [field]: value };
      for (const prepare of [
        () => f.schema.prepare({ preparationLimits }),
        () => f.schema.prepare({ preparationLimits, resources: f.resources }),
        () => f.resources.prepare(fixture.uri, { preparationLimits }),
      ]) {
        const result = prepare();
        assert.equal(result.status, "refused");
        assert.equal(result.failure.kind, "limit");
        assert.equal(result.failure.details.resource, code);
      }
      for (const register of [
        () =>
          new sdk.ExternalSchemas([[fixture.uri, f.value]], {
            limits: preparationLimits,
          }),
        () =>
          f.resources.with("urn:extra", f.value, { limits: preparationLimits }),
      ])
        assert.throws(register, (error) => {
          assert.ok(error instanceof sdk.SdkError);
          assert.equal(error.code, "resource");
          assert.deepEqual(error.details, { resource: code });
          assert.equal(Object.isFrozen(error.details), true);
          assert.equal(
            error.message,
            "external schema registration exceeded a preparation limit",
          );
          return true;
        });
      const retry = f.schema.prepare();
      assert.equal(retry.status, "ready");
      try {
        assert.equal(retry.schema.evaluate(7).status, "satisfies");
      } finally {
        retry.schema.dispose();
      }
    } finally {
      f.dispose();
    }
  });
}

test("every preparation number is validated on all routes and only work accepts null", () => {
  const f = owners();
  try {
    for (const { field } of fixture.cases) {
      for (const value of [
        -1,
        0.5,
        NaN,
        Infinity,
        2 ** 32,
        "1",
        undefined,
        ...(field === "maxSdkWorkUnits" ? [] : [null]),
      ]) {
        const limits = { [field]: value };
        for (const call of [
          () => f.schema.prepare({ preparationLimits: limits }),
          () => f.resources.prepare(fixture.uri, { preparationLimits: limits }),
          () => new sdk.ExternalSchemas([], { limits }),
          () => f.resources.with("urn:extra", f.value, { limits }),
        ])
          assert.throws(call, { code: "invalid-preparation-limits" });
      }
    }
    for (const limits of [null, [], 7, { unknown: 1 }])
      assert.throws(() => f.schema.prepare({ preparationLimits: limits }), {
        code: "invalid-preparation-limits",
      });
    const ready = f.schema.prepare({
      preparationLimits: { maxSdkWorkUnits: null, maxSchemaNodes: 0xffffffff },
    });
    assert.equal(ready.status, "ready");
    ready.schema.dispose();
  } finally {
    f.dispose();
  }
});

test("registration budgets apply to the whole set and stay separate from preparation", () => {
  const f = owners();
  try {
    const set = new sdk.ExternalSchemas([[fixture.uri, f.value]], {
      limits: { maxSchemaNodes: 1, maxIdentityBytes: fixture.uri.length },
    });
    try {
      assert.throws(
        () => set.with("urn:extra", f.value, { limits: { maxSchemaNodes: 1 } }),
        { code: "resource" },
      );
      assert.throws(
        () =>
          set.with("urn:extra", f.value, {
            limits: { maxIdentityBytes: fixture.uri.length },
          }),
        { code: "resource" },
      );
      const extended = set.with("urn:extra", f.value, {
        limits: { maxSchemaNodes: 2 },
      });
      extended.dispose();
      const prepared = set.prepare(fixture.uri);
      assert.equal(prepared.status, "ready");
      prepared.schema.dispose();
    } finally {
      set.dispose();
    }
  } finally {
    f.dispose();
  }
});

test("registration cancellation stops iteration and releases staged owners", () => {
  const f = owners();
  try {
    let iterated = false;
    const entries = {
      *[Symbol.iterator]() {
        iterated = true;
        yield [fixture.uri, f.value];
      },
    };
    assert.throws(
      () => new sdk.ExternalSchemas(entries, { signal: AbortSignal.abort() }),
      { code: "resource" },
    );
    assert.equal(iterated, false);
    const controller = new AbortController();
    function* cancelled() {
      yield [fixture.uri, f.value];
      controller.abort();
    }
    assert.throws(
      () => new sdk.ExternalSchemas(cancelled(), { signal: controller.signal }),
      { code: "resource" },
    );
    assert.throws(
      () =>
        f.resources.with("urn:extra", f.value, { signal: controller.signal }),
      { code: "resource" },
    );
    assert.equal(f.value.text, fixture.schema);
  } finally {
    f.dispose();
  }
});

test("preparation options do not leave stale handles after caller reentry", () => {
  const f = owners();
  try {
    assert.throws(
      () =>
        f.schema.prepare({
          get preparationLimits() {
            f.schema.dispose();
            return {};
          },
        }),
      { code: "disposed-handle" },
    );
    assert.throws(
      () =>
        f.resources.with("urn:extra", f.value, {
          get limits() {
            f.resources.dispose();
            return {};
          },
        }),
      { code: "disposed-handle" },
    );
  } finally {
    f.dispose();
  }
});
