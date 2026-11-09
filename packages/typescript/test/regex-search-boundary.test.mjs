import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { prepareInput } from "./conformant-contracts.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);

// The native route-counter regression pins compiled selection and Thompson
// counts for these exact public inputs. Literal endpoints contain no braces.
const atom = "[\u0081-\u{80001}]";
for (const [copies, nullable] of [
  [16_382, false],
  [16_379, true],
]) {
  for (const propertyName of [false, true]) {
    test(`dense ${nullable ? "nullable" : "nonnullable"} ${propertyName ? "patternProperties" : "pattern"} searches empty input in Wasm`, () => {
      const pattern = atom.repeat(copies) + (nullable ? "|" : "");
      const schema = propertyName
        ? { patternProperties: { [pattern]: false } }
        : { pattern };
      const document = sdk.Document.parse(
        JSON.stringify({
          openbindings: "0.2.0",
          operations: { op: { input: schema } },
        }),
      ).value;
      let prepared;
      try {
        const ready = prepareInput(document, "op");
        assert.equal(ready.status, "ready");
        prepared = ready.schema;
        const result = prepared.evaluate(propertyName ? { "": 0 } : "");
        assert.equal(
          result.status,
          (propertyName ? !nullable : nullable) ? "satisfies" : "fails",
        );
      } finally {
        prepared?.dispose();
        document.dispose();
      }
    });
  }
}
