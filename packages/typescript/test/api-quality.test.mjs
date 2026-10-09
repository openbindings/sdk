import test from "node:test";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import * as http from "../dist/http-discovery.js";
import { apiQualityCases } from "./api-quality-cases.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
test("accepted API contract: tagged states, exact/ordinary input, failure cleanup and shared realm", async () => {
  await apiQualityCases(sdk, http);
});
