import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { executionFrameCases } from "./execution-frames.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
test("original schema frames handle deep and large values through Wasm", () => {
  assert.equal(executionFrameCases(sdk).length, 8);
});
