import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import * as testing from "../dist/testing.js";
import { originalExplainCases } from "./original-explain.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const controls = JSON.parse(
  await readFile(
    new URL(
      "../../../crates/openbindings-json-schema-evaluator/src/semantic/explain/controls.json",
      import.meta.url,
    ),
    "utf8",
  ),
);
test("original explanations preserve independently fixed proofs and exact locations", () => {
  assert.equal(originalExplainCases(sdk, controls, testing).length, 42);
});
