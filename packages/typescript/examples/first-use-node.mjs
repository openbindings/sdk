// node node_modules/@openbindings/sdk/examples/first-use-node.mjs
import { readFile } from "node:fs/promises";
import { initialize } from "@openbindings/sdk";
import { firstUse } from "./first-use.mjs";

await initialize(
  await readFile(
    new URL(import.meta.resolve("@openbindings/sdk/openbindings.wasm")),
  ),
);
console.log(
  JSON.stringify(
    {
      accepted: firstUse({ id: 7 }),
      mismatch: firstUse({ id: 0 }),
      invalidInput: firstUse({ id: NaN }),
    },
    null,
    2,
  ),
);
