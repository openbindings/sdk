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
// d0 -> d1 -> ... -> d(links), the last an integer; with `id`, each link is a
// contained `$id` resource.
const chain = (links, id) => {
  const schemas = {};
  const name = (i) => (id ? `https://chain.test/d${i}` : `#/schemas/d${i}`);
  for (let i = 0; i < links; i++)
    schemas[`d${i}`] = {
      ...(id && { $id: name(i) }),
      $ref: name(i + 1),
    };
  schemas[`d${links}`] = { ...(id && { $id: name(links) }), type: "integer" };
  return document(schemas, { $ref: "#/schemas/d0" });
};
// Levels of contained resources whose two branches both reference the next
// level; `last` is the final level's body.
const diamonds = (levels, last) => {
  const uri = (name) => `https://diamond.test/${name}`;
  const schemas = {};
  for (let i = 0; i < levels; i++) {
    schemas[`r${i}`] = {
      $id: uri(`r${i}`),
      allOf: [{ $ref: uri(`a${i}`) }, { $ref: uri(`b${i}`) }],
    };
    for (const branch of ["a", "b"])
      schemas[`${branch}${i}`] = {
        $id: uri(`${branch}${i}`),
        $ref: uri(`r${i + 1}`),
      };
  }
  schemas[`r${levels}`] = { $id: uri(`r${levels}`), ...last };
  return document(schemas, { $ref: "#/schemas/r0" });
};
// `scopes` resources that each reference one shared resource requiring `names`
// property names; with `dynamic`, the root also declares a dynamic anchor that
// one of its own properties resolves.
const requiredFan = (scopes, names, dynamic) => {
  const uri = (name) => `https://fan.test/${name}`;
  const properties = {};
  for (let i = 0; i < scopes; i++) properties[`p${i}`] = { $ref: uri(`m${i}`) };
  if (dynamic) properties.self = { $dynamicRef: "#node" };
  const schemas = {
    top: {
      $id: uri("top"),
      ...(dynamic && { $dynamicAnchor: "node" }),
      type: "object",
      properties,
    },
  };
  for (let i = 0; i < scopes; i++)
    schemas[`m${i}`] = { $id: uri(`m${i}`), $ref: uri("leaf") };
  schemas.leaf = {
    $id: uri("leaf"),
    required: Array.from({ length: names }, (_, n) => `n${n}`),
  };
  return document(schemas, { $ref: "#/schemas/top" });
};
// `levels` levels whose two branches reference the next level; the first branch
// also declares the dynamic anchor `n<level>`, which the last level resolves, so
// each of the 2^levels paths reaches it in a dynamic context of its own.
const dynamicChoices = (levels) => {
  const uri = (name) => `https://choices.test/${name}`;
  const schemas = {};
  for (let i = 0; i < levels; i++) {
    const next = uri(`l${i + 1}`);
    schemas[`l${i}`] = {
      $id: uri(`l${i}`),
      anyOf: [{ $ref: uri(`a${i}`) }, { $ref: uri(`b${i}`) }],
    };
    schemas[`a${i}`] = {
      $id: uri(`a${i}`),
      $ref: next,
      $defs: { n: { $dynamicAnchor: `n${i}`, minimum: 0 } },
    };
    schemas[`b${i}`] = { $id: uri(`b${i}`), $ref: next };
  }
  const defs = {};
  for (let i = 0; i < levels; i++) defs[`n${i}`] = { $dynamicAnchor: `n${i}` };
  schemas[`l${levels}`] = {
    $id: uri(`l${levels}`),
    type: "integer",
    allOf: Array.from({ length: levels }, (_, i) => ({
      $dynamicRef: `#n${i}`,
    })),
    $defs: defs,
  };
  return document(schemas, { $ref: "#/schemas/l0" });
};
const integer = { type: "integer" };
const dynamicInteger = {
  $dynamicRef: "#leaf",
  $defs: { leaf: { $dynamicAnchor: "leaf", type: "integer" } },
};
const preparation = (text, limits) => {
  const parsed = sdk.Document.parse(text);
  assert.equal(parsed.status, "parsed");
  try {
    return prepareInput(parsed.value, "op", limits && { limits });
  } finally {
    parsed.value.dispose();
  }
};
const prepares = (text) => {
  const prepared = preparation(text);
  assert.equal(prepared.status, "ready");
  prepared.valueContract.dispose();
};
const refuses = (text) => {
  const prepared = preparation(text);
  assert.equal(prepared.status, "no-verdict");
  assert.equal(prepared.detail.reason, "limit-exceeded");
  assert.equal(prepared.detail.code, "program-specialization-limit");
  assert.equal(prepared.detail.location, null);
};

test("preparation compiles each schema once and bounds dynamic specialization through Wasm", () => {
  prepares(chain(5000, false));
  prepares(chain(100, true));
  // One resource, however many reference paths through resources reach a
  // schema; a dynamic anchor every path resolves alike changes nothing.
  prepares(diamonds(24, integer));
  prepares(diamonds(24, dynamicInteger));
  prepares(requiredFan(1000, 20000, true));
  prepares(requiredFan(1000, 20000, false));
  // Each path its own dynamic context: bounded.
  prepares(dynamicChoices(8));
  refuses(dynamicChoices(17));
});

test("the program and stack budgets scale what preparation and evaluation admit", () => {
  const limits = sdk.evaluatorLimits();
  assert.equal(limits.programBytes, 50331648);
  assert.equal(limits.stackBytes, 262144);
  assert.equal("evaluationDepth" in limits, false);
  assert.throws(
    () => preparation(dynamicChoices(1), { evaluationDepth: 1024 }),
    (e) => e.code === "invalid-evaluator-limits",
  );
  // 256 dynamic contexts reach the last of 8 levels, whose copies take about
  // 6.8 MiB: a smaller budget refuses them and names itself.
  const small = preparation(dynamicChoices(8), { programBytes: 4 << 20 });
  assert.equal(small.status, "no-verdict");
  assert.equal(small.detail.code, "program-specialization-limit");
  assert.ok(
    small.detail.message.endsWith(
      "by more than its 4 MiB program_bytes; raise program_bytes (programBytes in TypeScript)",
    ),
    small.detail.message,
  );
  const enough = preparation(dynamicChoices(8), { programBytes: 8 << 20 });
  assert.equal(enough.status, "ready");
  enough.valueContract.dispose();
  // A value nested deeper than the default stack admits is refused, and
  // decided with a larger stack.
  const nested = document(
    {},
    { type: "array", items: { $ref: "#/operations/op/input" } },
  );
  const deep = sdk.parseJson("[".repeat(400) + "]".repeat(400)).value;
  try {
    const defaults = preparation(nested);
    const refused = defaults.valueContract.validate(deep);
    assert.equal(refused.status, "no-verdict");
    assert.equal(refused.detail.code, "evaluation-depth-limit");
    assert.equal(
      refused.detail.message,
      "this value nests more than 340 levels, the deepest this value contract evaluates within the 1024 schema evaluations its 256 KiB stack_bytes holds at 256 bytes each; raise stack_bytes (stackBytes in TypeScript)",
    );
    defaults.valueContract.dispose();
    const larger = preparation(nested, { stackBytes: 512 * 1024 });
    assert.equal(larger.valueContract.validate(deep).status, "satisfies");
    larger.valueContract.dispose();
  } finally {
    deep.dispose();
  }
});
