import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { prepareInput } from "./conformant-contracts.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const document = (schemas, input) =>
  JSON.stringify({
    openbindings: "0.2.0",
    schemas,
    operations: { op: { input } },
  });
const preparation = (text, resources, cap, limits = {}) => {
  const parsed = sdk.Document.parse(text);
  assert.equal(parsed.status, "parsed");
  try {
    return prepareInput(parsed.value, "op", {
      resources,
      limits: { ...limits, schemaResources: cap },
    });
  } finally {
    parsed.value.dispose();
  }
};
const assertRefused = (prepared) => {
  assert.equal(prepared.status, "refused");
  assert.equal(prepared.failure.kind, "limit");
  assert.equal(prepared.failure.details.resource, "schema-resource-limit");
  assert.equal(prepared.failure.origin, null);
};
test("the default sets no resource cap", () => {
  assert.equal(sdk.evaluatorLimits().schemaResources, null);
});

test("a resource cap precedes a demanded local pattern limit", () => {
  const text = document({}, { pattern: "^(ab)+$" });
  const within = preparation(text, undefined, 1, { patternBytes: 4 });
  assert.equal(within.status, "ready");
  try {
    assert.equal(within.schema.evaluate(null).status, "satisfies");
    const result = within.schema.evaluate("ab");
    assert.equal(result.status, "no-verdict");
    assert.equal(result.detail.reason, "limit-exceeded");
    assert.equal(result.detail.code, "pattern-byte-limit");
  } finally {
    within.schema.dispose();
  }
  assertRefused(preparation(text, undefined, 0, { patternBytes: 4 }));
});

test("a contract's patterns are bounded together", () => {
  const text = document(
    {},
    {
      properties: {
        a: { pattern: "^aaaaaa$" },
        b: { patternProperties: { "^bbbbbb$": true } },
      },
    },
  );
  const within = preparation(text, undefined, null, { totalPatternBytes: 16 });
  assert.equal(within.status, "ready");
  within.schema.dispose();
  const over = preparation(text, undefined, null, { totalPatternBytes: 15 });
  assert.equal(over.status, "refused");
  assert.equal(over.failure.kind, "limit");
  assert.equal(over.failure.details.resource, "pattern-total-byte-limit");
  assert.equal(sdk.evaluatorLimits().totalPatternBytes, 4 * 1024 * 1024);
  // The total counts forms, which spell every set out: a two-byte `\S` passes
  // a 64-byte total.
  const spelled = document({}, { pattern: "\\S" });
  const short = preparation(spelled, undefined, null, {
    totalPatternBytes: 64,
  });
  assert.equal(short.status, "refused");
  assert.equal(short.failure.details.resource, "pattern-total-byte-limit");
  const ample = preparation(spelled, undefined, null, {
    totalPatternBytes: 1024,
  });
  assert.equal(ample.status, "ready");
  ample.schema.dispose();
});

// Immutable programs share a defined representation footprint. Construction
// has a separate aggregate allowance; caches belong to value calls.
test("pattern preparation bounds construction and unique program storage", () => {
  assert.equal(sdk.evaluatorLimits().totalPatternMemory, 64 * 1024 * 1024);
  const refusal = (state, code) => {
    assert.equal(state.status, "refused");
    assert.equal(state.failure.kind, "limit");
    assert.equal(state.failure.details.resource, code);
  };
  const ready = (state) => {
    assert.equal(state.status, "ready");
    return state.schema;
  };
  const names = (count) =>
    Object.fromEntries(
      Array.from({ length: count }, (_, i) => [`^.{1,10000}${i}$`, false]),
    );
  refusal(
    preparation(
      document({}, { patternProperties: names(40) }),
      undefined,
      null,
    ),
    "pattern-construction-helper-payload",
  );
  const small = Object.fromEntries(
    Array.from({ length: 100 }, (_, i) => [`s${i}`, { pattern: `a${i}|b` }]),
  );
  for (const properties of [{}, small]) {
    const contract = ready(
      preparation(
        document(
          {},
          {
            patternProperties: names(1),
            properties,
          },
        ),
        undefined,
        null,
      ),
    );
    try {
      assert.equal(contract.evaluateJson('{"a0":1}').status, "fails");
      assert.equal(contract.evaluateJson("{}").status, "satisfies");
    } finally {
      contract.dispose();
    }
  }
  const three = { totalPatternMemory: 3 * 64 * 1024 };
  const compiled = (count) =>
    Array.from({ length: count }, (_, i) => ({ pattern: `a${i}|b` }));
  for (const prefixItems of [
    [...compiled(2), { pattern: "^a+$" }],
    [...compiled(3), { pattern: "^a+$" }],
    compiled(4),
  ]) {
    ready(
      preparation(document({}, { prefixItems }), undefined, null, three),
    ).dispose();
  }
  const quarter = { totalPatternMemory: 256 * 1024 };
  ready(
    preparation(
      document({}, { patternProperties: { "^.{1,100}x$": false } }),
      undefined,
      null,
      quarter,
    ),
  ).dispose();
  for (const schema of [
    { patternProperties: { "^.{1,1000}x$": false } },
    { pattern: "^.{1,1000}x$" },
  ]) {
    refusal(
      preparation(document({}, schema), undefined, null, quarter),
      "pattern-total-memory-limit",
    );
  }
  const repeated = {
    prefixItems: Array.from({ length: 100 }, () => ({ pattern: "a|b" })),
  };
  const shared = ready(preparation(document({}, repeated), undefined, null));
  try {
    assert.equal(shared.evaluate(["a", "b"]).status, "satisfies");
    assert.equal(shared.evaluate(["z"]).status, "fails");
  } finally {
    shared.dispose();
  }
});

test("expanded pattern preparation preserves default capacity and exact sharing", () => {
  const pattern = (i) => "\\S".repeat(5000) + `z${i}`;
  const input = (count, repeated = false) => ({
    properties: Object.fromEntries(
      Array.from({ length: count }, (_, i) => [
        `p${i}`,
        { pattern: pattern(repeated ? 0 : i) },
      ]),
    ),
  });
  for (const [count, repeated] of [
    [1, false],
    [4, false],
    [380, true],
  ]) {
    const result = preparation(
      document({}, input(count, repeated)),
      undefined,
      null,
    );
    assert.equal(result.status, "ready");
    const prepared = result.schema;
    try {
      assert.equal(prepared.evaluate({}).status, "satisfies");
    } finally {
      prepared.dispose();
    }
  }
  const fifth = preparation(document({}, input(5)), undefined, null);
  assert.equal(fifth.status, "refused");
  assert.equal(fifth.failure.kind, "limit");
  assert.equal(fifth.failure.details.resource, "pattern-total-byte-limit");
});
