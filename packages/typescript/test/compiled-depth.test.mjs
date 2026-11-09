import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { compiledDepthCases } from "./compiled-depth.mjs";

await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
test("default Wasm depth walk crosses compiled fallback and reaches the parse ceiling", (t) => {
  const result = compiledDepthCases(sdk);
  assert.equal(result.rows.length, 7);
  t.diagnostic(JSON.stringify(result));
});
