import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { patternCacheCases } from "./pattern-cache.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
test("original patterns handle Unicode identifiers and broad epsilon closures in Wasm", () => {
  assert.equal(patternCacheCases(sdk).length, 9);
});
