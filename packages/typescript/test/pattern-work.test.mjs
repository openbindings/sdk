import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";

await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);

test("a pattern charge above 2^32 needs a caller-selected work cap", () => {
  const document = sdk.Document.parse(
    '{"openbindings":"0.2.0","operations":{"op":{"input":{"pattern":"^(?:a{1,200}){1,200}$"}}}}',
  ).value;
  const proof = document.validate().document;
  const operation = proof.operation("op");
  const schema = operation.input();
  // Fail the anchored pattern at its first byte. Its length-times-program
  // charge exceeds 2^32 while the real search finishes immediately.
  const value = "b" + "a".repeat(20_000);
  try {
    for (const [options, expected] of [
      [{}, "fails"],
      [{ limits: { evaluationSdkWorkUnits: null } }, "fails"],
      [{ limits: { evaluationSdkWorkUnits: 4_294_967_295 } }, "no-verdict"],
    ]) {
      const setup = schema.prepare({
        limits: { patternSearchWorkUnits: null, ...options.limits },
      });
      assert.equal(setup.status, "ready");
      try {
        const outcome = setup.schema.evaluate(value);
        assert.equal(outcome.status, expected);
        if (expected === "no-verdict")
          assert.equal(outcome.detail.code, "evaluation-work-limit");
      } finally {
        setup.schema.dispose();
      }
    }
  } finally {
    schema.dispose();
    operation.dispose();
    proof.dispose();
    document.dispose();
  }
});

function usingSchema(pattern, body) {
  const document = sdk.Document.parse(
    JSON.stringify({
      openbindings: "0.2.0",
      operations: { op: { input: { pattern } } },
    }),
  ).value;
  const proof = document.validate().document;
  const operation = proof.operation("op");
  const schema = operation.input();
  try {
    body(schema);
  } finally {
    schema.dispose();
    operation.dispose();
    proof.dispose();
    document.dispose();
  }
}

test("hostile searches are refused by default and run with the ceiling removed", () => {
  for (const pattern of [
    "^(?:(?:a{1,200}){1,200}|a)*$",
    "^(?:a{1,200}){1,200}$",
  ]) {
    usingSchema(pattern, (schema) => {
      const bounded = schema.prepare();
      assert.equal(bounded.status, "ready");
      try {
        for (const n of [2_000, 20_000, 50_000]) {
          const outcome = bounded.schema.evaluate("a".repeat(n) + "b");
          assert.equal(outcome.status, "no-verdict");
          assert.equal(outcome.detail.reason, "limit-exceeded");
          assert.equal(outcome.detail.code, "pattern-search-work-limit");
        }
      } finally {
        bounded.schema.dispose();
      }
      const unbounded = schema.prepare({
        limits: { patternSearchWorkUnits: null },
      });
      assert.equal(unbounded.status, "ready");
      try {
        assert.equal(
          unbounded.schema.evaluate("a".repeat(2_000) + "b").status,
          "fails",
        );
      } finally {
        unbounded.schema.dispose();
      }
    });
  }
});

test("search ceiling uses Wasm defaults and the optional numeric limit contract", () => {
  assert.equal(sdk.evaluatorLimits().patternSearchWorkUnits, 250_000_000);
  usingSchema("^abc$", (schema) => {
    for (const cap of [
      null,
      24,
      25,
      4_294_967_295,
      sdk.evaluatorLimits().patternSearchWorkUnits,
    ]) {
      const prepared = schema.prepare({
        limits: { patternSearchWorkUnits: cap },
      });
      assert.equal(prepared.status, "ready");
      try {
        assert.equal(prepared.schema.evaluate("abc").status, "satisfies");
      } finally {
        prepared.schema.dispose();
      }
    }
    const prepared = schema.prepare({ limits: { patternSearchWorkUnits: 23 } });
    assert.equal(prepared.status, "ready");
    try {
      const outcome = prepared.schema.evaluate("abc");
      assert.equal(outcome.status, "no-verdict");
      assert.equal(outcome.detail.code, "pattern-search-work-limit");
      assert.equal(prepared.schema.evaluate(1).status, "satisfies");
    } finally {
      prepared.schema.dispose();
    }
    for (const cap of [-1, 0.5, 4_294_967_296, NaN, Infinity, "1", undefined]) {
      assert.throws(
        () => schema.prepare({ limits: { patternSearchWorkUnits: cap } }),
        (error) => error.code === "invalid-evaluator-limits",
      );
    }
  });
});
