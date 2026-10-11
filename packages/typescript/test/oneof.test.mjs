import test from "node:test";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { oneOfCases } from "./oneof-cases.mjs";
const root = new URL(
  "../../../crates/openbindings-json-schema-evaluator/tests/fixtures/oneof/",
  import.meta.url,
);
const text = (name) => readFile(new URL(name, root), "utf8");
const fixtures = {
  c22: await text("C22.json"),
  c22Cases: JSON.parse(await text("C22-cases.json")),
  controls: JSON.parse(await text("owner-cases.json")),
};
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
test("oneOf frozen decisions, summaries and lifetime through Wasm", () =>
  oneOfCases(sdk, fixtures));
