// node node_modules/@openbindings/sdk/examples/first-use-node.mjs
import { initialize } from "@openbindings/sdk";
import { firstUse } from "./first-use.mjs";

await initialize();
console.log(
  JSON.stringify(
    {
      accepted: firstUse({ id: 7 }),
      fails: firstUse({ id: 0 }),
      invalidInput: firstUse({ id: NaN }),
    },
    null,
    2,
  ),
);
