// node examples/node-service.mjs; installed package imports only.
import { readFile } from "node:fs/promises";
import { initialize, parseJson } from "@openbindings/sdk";
import { prepareService, ValidationService } from "./service.mjs";

await initialize(
  await readFile(
    new URL(import.meta.resolve("@openbindings/sdk/openbindings.wasm")),
  ),
);
const schema = parseJson('{"type":"integer"}');
if (schema.status !== "parsed")
  throw new Error("Example schema failed admission.");
let service;
try {
  const setup = prepareService(
    '{"openbindings":"0.2.0","operations":{"lookup":{"input":{"$ref":"https://schema.example/input"}}}}',
    [["https://schema.example/input", schema.value]],
  );
  if (setup.status !== "ready") throw new Error(JSON.stringify(setup));
  service = new ValidationService(setup.contract);
  console.log(
    JSON.stringify({
      ordinary: service.check(7),
      exact: await service.checkBytes(Promise.resolve("9007199254740993")),
    }),
  );
} finally {
  service?.dispose();
  schema.value.dispose();
}
