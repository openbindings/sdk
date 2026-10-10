import test from "node:test";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { diagnosticDetailsCases } from "./diagnostic-details-cases.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
test("opt-in exact schema facts, atomic budget truncation and source retention", () =>
  diagnosticDetailsCases(sdk));
