import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
import { liveStorageOwners } from "../dist/testing.js";
await sdk.initialize();

function prepare(text) {
  const source = sdk.parseJson(text).value;
  const resources = new sdk.ExternalSchemas([["urn:explain-json", source]]);
  try {
    const result = resources.prepare("urn:explain-json");
    assert.equal(result.status, "ready");
    return result.schema;
  } finally {
    resources.dispose();
    source.dispose();
  }
}

// Initialize shared meta-schema storage before counting per-call owners.
prepare("true").dispose();

test("explainJson preserves exact numbers and returns detached failure problems", () => {
  const before = liveStorageOwners();
  const schema = prepare(
    '{"type":"object","required":["n"],"properties":{"n":{"enum":[9007199254740993]}}}',
  );
  const retained = liveStorageOwners();
  let failed;
  try {
    for (const text of [
      '{"n":9007199254740993}',
      '{"n":9007199254740992}',
      "{}",
      '{"n":1,"n":2}',
    ]) {
      const input = sdk.parseJson(text).value;
      const expected = schema.explain(input);
      input.dispose();
      for (const source of [text, new TextEncoder().encode(text)]) {
        const result = schema.explainJson(source);
        assert.deepEqual(result, expected);
        assert.equal(liveStorageOwners(), retained);
        assert.equal(Object.hasOwn(result, "dispose"), false);
      }
    }
    assert.equal(
      schema.explainJson('{"n":9007199254740993}').status,
      "satisfies",
    );
    failed = schema.explainJson('{"n":9007199254740992}');
    assert.equal(failed.status, "fails");
    assert.equal(failed.problemsComplete, true);
    assert.equal(failed.problems.length, 1);
    assert.equal(failed.problems[0].instance, "/n");
    assert.deepEqual(failed.problems[0].details, {
      kind: "enum",
      choices: ["9007199254740993"],
    });
  } finally {
    schema.dispose();
  }
  assert.equal(failed.problems[0].instance, "/n");
  assert.equal(liveStorageOwners(), before);
  assert.throws(() => schema.explainJson("{}"), { code: "disposed-handle" });
});

test("explainJson shares evaluateJson admission defaults and cancellation precedence", () => {
  const schema = prepare('{"type":"integer"}');
  const retained = liveStorageOwners();
  const signal = AbortSignal.abort();
  try {
    for (const input of [
      "",
      "7 x",
      "{",
      new Uint8Array([0xff]),
      new Uint8Array([0xef, 0xbb, 0xbf, 0x37]),
      "[".repeat(10_001) + "]".repeat(10_001),
      " ".repeat(32 * 1024 * 1024) + "0",
      "[" + "0,".repeat(1_000_000) + "0]",
    ]) {
      const expected = schema.evaluateJson(input);
      assert.equal(expected.status, "input-error");
      assert.deepEqual(schema.explainJson(input), expected);
      assert.deepEqual(schema.explainJson(input, { signal }), expected);
      assert.equal(liveStorageOwners(), retained);
    }
    assert.deepEqual(
      schema.explainJson("7", { signal }),
      schema.evaluateJson("7", { signal }),
    );
    assert.equal(
      schema.explainJson('{"a":1,"a":2}').detail.code,
      "duplicate-instance-members",
    );
    for (const input of [7, {}, "\ud800"])
      assert.throws(() => schema.explainJson(input), TypeError);
    assert.throws(() => schema.explainJson("7", { unknown: true }), {
      code: "invalid-work-options",
    });
  } finally {
    schema.dispose();
  }
});
