import test from "node:test";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { diagnosticPolicyCases } from "./diagnostic-policy-cases.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
test("diagnostic fidelity, safe messages and reason coherence", () =>
  diagnosticPolicyCases(sdk));
