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
