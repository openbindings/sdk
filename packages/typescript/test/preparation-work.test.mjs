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
const integer = { type: "integer" };
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
  prepares(chain(1779, false));
  refuses(chain(1780, false));
  prepares(chain(402, true));
  refuses(chain(403, true));
  prepares(diamonds(8, integer));
  refuses(diamonds(9, integer));
});
