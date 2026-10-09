import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { runCases } from "./authoring-cases.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
for (const id of [
  "A01",
  "A02",
  "A03",
  "A04",
  "A05",
  "A06",
  "A07",
  "A08",
  "A09",
  "A10",
  "A11",
  "A12",
  "P01",
  "P02",
  "X01",
  "X02",
]) {
  test(id, async () => {
    const [row] = await runCases(sdk, [id]);
    assert.equal(row.status, "pass", JSON.stringify(row));
  });
}
