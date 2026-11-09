import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { exactPointerCases } from "./exact-pointers.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
test("direct exact pointers preserve deep names and retained subtrees in Wasm", () => {
  assert.equal(exactPointerCases(sdk).length, 4);
});
