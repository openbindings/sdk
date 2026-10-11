import test from "node:test";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { plannerLocationCases } from "./planner-location-cases.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
test("planner locations and actual production limits through Wasm", () =>
  plannerLocationCases(sdk));
