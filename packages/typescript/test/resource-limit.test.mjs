import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";
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
const integer = { type: "integer" };
// An input that references each of `count` supplied integer resources.
const supplied = (count) => {
  const uri = (i) => `https://set.test/r${i}`;
  const input = {
    allOf: Array.from({ length: count }, (_, i) => ({ $ref: uri(i) })),
  };
  const values = Array.from({ length: count }, () =>
    sdk.ExactJson.from(integer),
  );
  try {
    const resources = new sdk.ExternalSchemas(
      values.map((value, i) => [uri(i), value]),
    );
    return { text: document({}, input), resources };
  } finally {
    for (const value of values) value.dispose();
  }
};
// d0 -> d1 -> ... -> d(links), one contained `$id` resource per schema.
const idChain = (links) => {
  const uri = (i) => `https://chain.test/d${i}`;
  const schemas = {};
  for (let i = 0; i < links; i++)
    schemas[`d${i}`] = { $id: uri(i), $ref: uri(i + 1) };
  schemas[`d${links}`] = { $id: uri(links), ...integer };
  return document(schemas, { $ref: "#/schemas/d0" });
};
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
// A cap of `reached` admits the contract, and so does no cap; one less refuses
// it. The admitted contract decides `value`, which satisfies it.
const assertReaches = (text, resources, reached, value = 7) => {
  for (const cap of [reached, null]) {
    const prepared = preparation(text, resources, cap);
    assert.equal(prepared.status, "ready", `cap ${cap}`);
    assert.equal(prepared.schema.evaluate(value).status, "satisfies");
    prepared.schema.dispose();
  }
  assertRefused(preparation(text, resources, reached - 1));
};

test("the default sets no resource cap", () => {
  assert.equal(sdk.evaluatorLimits().schemaResources, null);
});

test("a resource cap counts reached resources as natively", () => {
  // The document resource alone.
  assertReaches(document({}, integer), undefined, 1);
  // The document resource and each supplied resource.
  const forty = supplied(40);
  try {
    assertReaches(forty.text, forty.resources, 41);
    // Supplied resources the contract does not reach are not counted.
    assertReaches(document({}, integer), forty.resources, 1);
  } finally {
    forty.resources.dispose();
  }
  // The document resource and each contained `$id` resource.
  assertReaches(idChain(30), undefined, 32);
  // The document resource, the 2020-12 meta-schema and the seven vocabulary
  // meta-schemas it references.
  assertReaches(
    document({}, { $ref: "https://json-schema.org/draft/2020-12/schema" }),
    undefined,
    9,
    integer,
  );
  assertRefused(preparation(document({}, integer), undefined, 0));
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
  const minimum = (schema) => {
    const text = document({}, schema);
    let below = 0,
      enough = 1024 * 1024;
    ready(
      preparation(text, undefined, null, { totalPatternMemory: enough }),
    ).dispose();
    while (enough - below > 1) {
      const middle = below + Math.floor((enough - below) / 2);
      const state = preparation(text, undefined, null, {
        totalPatternMemory: middle,
      });
      if (state.status === "ready") {
        state.schema.dispose();
        enough = middle;
      } else {
        refusal(state, "pattern-total-memory-limit");
        below = middle;
      }
    }
    refusal(
      preparation(text, undefined, null, { totalPatternMemory: enough - 1 }),
      "pattern-total-memory-limit",
    );
    return enough;
  };
  const one = minimum({ pattern: "a|b" });
  const repeated = {
    prefixItems: Array.from({ length: 100 }, () => ({ pattern: "a|b" })),
  };
  assert.equal(minimum(repeated), one);
  const shared = ready(
    preparation(document({}, repeated), undefined, null, {
      totalPatternMemory: one,
    }),
  );
  try {
    assert.equal(shared.evaluate(["a", "b"]).status, "satisfies");
    assert.equal(shared.evaluate(["z"]).status, "fails");
  } finally {
    shared.dispose();
  }
});

test("large resource requests remain bounded before registration", () => {
  // The TypeScript facade uses the fixed preparation defaults. At 8,000
  // resources their shared work allowance can refuse before the resource cap.
  // Keep both original workloads and distinguish these actual failure causes.
  const fastest = (count) => {
    const { text, resources } = supplied(count);
    try {
      let best = Infinity;
      for (let run = 0; run < 3; run++) {
        const start = performance.now();
        const state = preparation(text, resources, count);
        if (count === 2000) assertRefused(state);
        else {
          assert.equal(state.status, "refused");
          assert.equal(state.failure.kind, "limit");
          assert.equal(
            state.failure.details.resource,
            "preparation-sdk-work-units",
          );
        }
        best = Math.min(best, performance.now() - start);
      }
      return best;
    } finally {
      resources.dispose();
    }
  };
  const small = fastest(2000);
  const large = fastest(8000);
  assert.ok(large / small < 8, `${small} ms -> ${large} ms`);
});
