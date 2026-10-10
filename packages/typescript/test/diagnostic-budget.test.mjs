import test from "node:test";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { diagnosticBudgetCases } from "./diagnostic-budget-cases.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
test("bounded value diagnostics and maintained amplification control", () =>
  diagnosticBudgetCases(sdk));
