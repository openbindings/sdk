import test from "node:test";
import { readFile } from "node:fs/promises";
import * as sdk from "@openbindings/sdk";
import { fixedDiagnosticCases } from "./fixed-diagnostic-cases.mjs";

await sdk.initialize(
  await readFile(
    new URL(import.meta.resolve("@openbindings/sdk/openbindings.wasm")),
  ),
);
test("public fixed-schema diagnostics explain safe repairs at original locations", () => {
  fixedDiagnosticCases(sdk);
});
