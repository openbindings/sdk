import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { referencePreparationCases } from "./reference-preparation.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
test("original repeated reference preparation fits unchanged Wasm limits", () => {
  assert.equal(referencePreparationCases(sdk).length, 4);
});
