import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
await sdk.initialize();
const cases = JSON.parse(
  await readFile(
    new URL(
      "../../../crates/openbindings-json-schema-evaluator/tests/fixtures/supplied-roots.json",
      import.meta.url,
    ),
    "utf8",
  ),
);
for (const fixture of cases) {
  test(`supplied root agrees with Rust: ${fixture.name}`, () => {
    const owners = [];
    const own = (value) => (owners.push(value), value);
    try {
      const entries = [
        [fixture.uri, fixture.schema],
        ...Object.entries(fixture.resources ?? {}),
      ];
      const resources = own(
        new sdk.ExternalSchemas(
          entries.map(([uri, schema]) => [
            uri,
            own(sdk.ExactJson.from(schema)),
          ]),
        ),
      );
      const preparation = resources.prepare(fixture.uri);
      assert.equal(preparation.status, "ready");
      const prepared = own(preparation.schema);
      for (const row of fixture.values) {
        const outcome = prepared.evaluate(row.value);
        const explained = prepared.explain(row.value);
        assert.equal(outcome.status, row.status);
        assert.equal(explained.status, row.status);
        if (explained.status === "fails") {
          assert.equal(explained.problemsComplete, true);
          assert.ok(explained.problems.length > 0);
          for (const problem of explained.problems) {
            assert.equal(
              problem.schema.sourceUri,
              row.source_uri ?? fixture.uri,
            );
            assert.equal(problem.schema.pointer, row.pointer);
            assert.ok(resources.locate(problem.schema));
          }
        }
      }
      const before = prepared.evaluate(fixture.values[0].value);
      resources.dispose();
      assert.deepEqual(prepared.evaluate(fixture.values[0].value), before);
    } finally {
      for (const owner of owners.reverse()) owner.dispose();
    }
  });
}

test("supplied preparation distinguishes missing roots, invalid schemas, limits and cancellation", () => {
  const uri = "https://private.example/root";
  const validUri = "https://private.example/valid";
  const schema = sdk.ExactJson.from({
    $id: "https://private.example/declared",
    type: 5,
  });
  const valid = sdk.ExactJson.from(true);
  const resources = new sdk.ExternalSchemas([
    [uri, schema],
    [validUri, valid],
  ]);
  const empty = new sdk.ExternalSchemas();
  try {
    for (const missing of [
      "https://private.example/missing",
      "https://private.example/declared",
    ]) {
      const result = resources.prepare(missing);
      assert.equal(result.status, "refused");
      assert.equal(result.failure.kind, "resolution");
      assert.deepEqual(result.failure.details.cause, {
        kind: "resource-unavailable",
        details: { uri: missing },
      });
    }
    const invalid = resources.prepare(uri);
    assert.equal(invalid.status, "refused");
    assert.equal(invalid.failure.kind, "invalid-external-schema");
    assert.equal(invalid.failure.origin.sourceUri, uri);
    const signal = AbortSignal.abort();
    assert.equal(
      resources.prepare(validUri, { signal }).failure.kind,
      "cancelled",
    );
    assert.equal(empty.prepare(uri).failure.kind, "resolution");
    assert.throws(
      () => resources.prepare(uri, { unknown: true }),
      /Unknown work option/,
    );
    assert.throws(() =>
      resources.prepare(uri, { limits: { patternBytes: -1 } }),
    );
    const limited = resources.prepare(validUri, {
      limits: { schemaResources: 0 },
    });
    assert.equal(limited.status, "refused");
    assert.equal(limited.failure.kind, "limit");
    const retried = resources.prepare(validUri);
    assert.equal(retried.status, "ready");
    retried.schema.dispose();
  } finally {
    resources.dispose();
    schema.dispose();
    valid.dispose();
    empty.dispose();
  }
});
