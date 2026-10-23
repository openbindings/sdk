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
    const resources = new sdk.SchemaResources(
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
  assert.equal(prepared.status, "no-verdict");
  assert.equal(prepared.detail.reason, "limit-exceeded");
  assert.equal(prepared.detail.code, "schema-resource-limit");
  assert.equal(prepared.detail.location, null);
};
// A cap of `reached` admits the contract, and so does no cap; one less refuses
// it. The admitted contract decides `value`, which satisfies it.
const assertReaches = (text, resources, reached, value = 7) => {
  for (const cap of [reached, null]) {
    const prepared = preparation(text, resources, cap);
    assert.equal(prepared.status, "ready", `cap ${cap}`);
    assert.equal(prepared.valueContract.validate(value).outcome, "satisfies");
    prepared.valueContract.dispose();
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

test("a resource cap refuses before preparing any resource", () => {
  // Each resource's patterns are admitted as it is registered, so a pattern
  // over the byte limit refuses a contract within the cap. Over the cap, the
  // same contract refuses for its resources before any of them is prepared.
  const text = document({}, { type: "string", pattern: "^(ab)+$" });
  const within = preparation(text, undefined, 1, { patternBytes: 4 });
  assert.equal(within.status, "no-verdict");
  assert.equal(within.detail.code, "pattern-byte-limit");
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
  within.valueContract.dispose();
  const over = preparation(text, undefined, null, { totalPatternBytes: 15 });
  assert.equal(over.status, "no-verdict");
  assert.equal(over.detail.reason, "limit-exceeded");
  assert.equal(over.detail.code, "pattern-total-byte-limit");
  assert.equal(sdk.evaluatorLimits().totalPatternBytes, 4 * 1024 * 1024);
  // The total counts forms, which spell every set out: a two-byte `\S` passes
  // a 64-byte total.
  const spelled = document({}, { pattern: "\\S" });
  const short = preparation(spelled, undefined, null, {
    totalPatternBytes: 64,
  });
  assert.equal(short.status, "no-verdict");
  assert.equal(short.detail.code, "pattern-total-byte-limit");
  const ample = preparation(spelled, undefined, null, {
    totalPatternBytes: 1024,
  });
  assert.equal(ample.status, "ready");
  ample.valueContract.dispose();
});

// A contract's compiled patterns share its pattern memory, with the same
// refusals, codes, locations and messages as the Rust evaluator: each compiled
// pattern first compiles within 48 KiB, those that need more share what that
// leaves, a name that does not fit what it is given refuses preparation, and a
// `pattern` value compiled when a string first reaches it gives that string no
// verdict.
test("a contract's compiled patterns share its pattern memory", () => {
  assert.equal(sdk.evaluatorLimits().totalPatternMemory, 64 * 1024 * 1024);
  const at = "/operations/op/input";
  const assertMemoryRefusal = (prepared, pointer) => {
    assert.equal(prepared.status, "no-verdict");
    assert.equal(prepared.detail.reason, "limit-exceeded");
    assert.equal(prepared.detail.code, "pattern-total-memory-limit");
    assert.match(prepared.detail.location.pointer, pointer);
    return prepared.detail.message;
  };
  const ready = (prepared) => {
    assert.equal(prepared.status, "ready");
    return prepared.valueContract;
  };
  const raise = "raise total_pattern_memory (totalPatternMemory in TypeScript)";
  // Forty names of the form `^.{1,10000}N$` need more together than the
  // memory leaves; one alone compiles and decides.
  const names = (count) =>
    Object.fromEntries(
      Array.from({ length: count }, (_, i) => [`^.{1,10000}${i}$`, false]),
    );
  const forty = assertMemoryRefusal(
    preparation(
      document({}, { patternProperties: names(40) }),
      undefined,
      null,
    ),
    new RegExp(`^${at}/patternProperties$`),
  );
  assert.match(forty, /; the 40 that need more share it equally, but this one/);
  assert.ok(forty.endsWith(raise));
  const alone = ready(
    preparation(document({}, { patternProperties: names(1) }), undefined, null),
  );
  assert.equal(alone.validateJson('{"a0":1}').outcome, "fails");
  assert.equal(alone.validateJson("{}").outcome, "satisfies");
  alone.dispose();
  // Small patterns elsewhere leave the large name its outcome.
  const small = Object.fromEntries(
    Array.from({ length: 100 }, (_, i) => [`s${i}`, { pattern: `a${i}|b` }]),
  );
  const among = ready(
    preparation(
      document({}, { patternProperties: names(1), properties: small }),
      undefined,
      null,
    ),
  );
  assert.equal(among.validateJson('{"a0":1}').outcome, "fails");
  among.dispose();
  // Three patterns' 48 KiB admit three compiled patterns; run matchers do
  // not count.
  const three = { totalPatternMemory: 3 * 64 * 1024 };
  const compiled = (count) =>
    Array.from({ length: count }, (_, i) => ({ pattern: `a${i}|b` }));
  ready(
    preparation(
      document({}, { prefixItems: [...compiled(3), { pattern: "^a+$" }] }),
      undefined,
      null,
      three,
    ),
  ).dispose();
  assert.equal(
    assertMemoryRefusal(
      preparation(
        document({}, { prefixItems: compiled(4) }),
        undefined,
        null,
        three,
      ),
      new RegExp(`^${at}/prefixItems/[0-9]+/pattern$`),
    ),
    "this value contract compiles more than 3 regular expressions, as many as its 192 KiB " +
      "total_pattern_memory holds at 48 KiB each once their search caches take a quarter; " +
      raise,
  );
  // One pattern in 256 KiB compiles `^.{1,100}x$` but not `^.{1,1000}x$`.
  const quarter = { totalPatternMemory: 256 * 1024 };
  ready(
    preparation(
      document({}, { patternProperties: { "^.{1,100}x$": false } }),
      undefined,
      null,
      quarter,
    ),
  ).dispose();
  assertMemoryRefusal(
    preparation(
      document({}, { patternProperties: { "^.{1,1000}x$": false } }),
      undefined,
      null,
      quarter,
    ),
    new RegExp(`^${at}/patternProperties$`),
  );
  const lazy = ready(
    preparation(
      document({}, { pattern: "^.{1,1000}x$" }),
      undefined,
      null,
      quarter,
    ),
  );
  const unmatched = lazy.validateJson('"ax"');
  assert.equal(unmatched.outcome, "no-verdict");
  assert.equal(unmatched.detail.code, "pattern-total-memory-limit");
  assert.equal(
    unmatched.detail.message,
    "this value contract's one compiled regular expression holds 48 KiB of its 256 KiB " +
      "total_pattern_memory and 64 KiB for its search cache, which leaves 144 KiB; this " +
      "one, which needs more, may take it all, but does not compile within the 192 KiB " +
      "that gives it; " +
      raise,
  );
  lazy.dispose();
});

test("a resource cap refusal takes time linear in the resources", () => {
  // Registering resources takes time quadratic in their number; a refusal
  // before registration grows with the projection, about fourfold here.
  const fastest = (count) => {
    const { text, resources } = supplied(count);
    try {
      let best = Infinity;
      for (let run = 0; run < 3; run++) {
        const start = performance.now();
        assertRefused(preparation(text, resources, count));
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
