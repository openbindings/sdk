import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { syntaxFactCases } from "./syntax-facts.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const fixture = JSON.parse(
  await readFile(
    new URL(
      "../../../crates/openbindings-json-schema-evaluator/src/semantic/construction/shared-meta-input.json",
      import.meta.url,
    ),
    "utf8",
  ),
);
test("shared syntax facts preserve complete schema admission in Wasm", () => {
  assert.equal(syntaxFactCases(sdk, fixture).length, 7);
});
