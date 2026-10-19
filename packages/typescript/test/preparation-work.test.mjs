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
// property names; with `dynamic`, a dynamic anchor keeps the shared resource
// compiled once per resource path.
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
const integer = { type: "integer" };
const dynamicInteger = {
  $dynamicRef: "#leaf",
  $defs: { leaf: { $dynamicAnchor: "leaf", type: "integer" } },
};
const preparation = (text) => {
  const parsed = sdk.Document.parse(text);
  assert.equal(parsed.status, "parsed");
  try {
    return prepareInput(parsed.value, "op");
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
  assert.equal(prepared.detail.code, "preparation-work-limit");
  assert.equal(prepared.detail.location, null);
};

test("preparation work refuses from the native boundaries through Wasm", () => {
  prepares(chain(23496, false));
  refuses(chain(23497, false));
  prepares(chain(100, true));
  // Without a dynamic anchor every level compiles once; with one, per path.
  prepares(diamonds(24, integer));
  prepares(diamonds(12, dynamicInteger));
  refuses(diamonds(13, dynamicInteger));
  // A long `required` list is charged each time it compiles.
  prepares(requiredFan(103, 20000, true));
  refuses(requiredFan(104, 20000, true));
  prepares(requiredFan(1000, 20000, false));
});
