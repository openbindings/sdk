import test from "node:test";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { partialResourceCases } from "./partial-resource-cases.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
test("partial resource proofs and phase migration through the Wasm facade", () =>
  partialResourceCases(sdk));
