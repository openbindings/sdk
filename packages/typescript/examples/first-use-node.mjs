// node node_modules/@openbindings/sdk/examples/first-use-node.mjs
import { initialize } from "@openbindings/sdk";
import { firstUse } from "./first-use.mjs";

await initialize();
// firstUse calls explain so failures include instance pointers and schema facts.
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
