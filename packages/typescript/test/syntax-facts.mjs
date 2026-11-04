import { prepareInput } from "./conformant-contracts.mjs";

// The fixture uses two supported custom profiles selecting a shared standard
// helper. It forces the original engine through public preparation on each host.
export function syntaxFactCases(sdk, fixture) {
  const results = [];
  const helper = "https://facts.test/helper";
  const patches = [
    ["shared-helper", null],
    ["unused-definition", { $defs: { unused: { type: 17 } } }],
    ["content-schema", { contentSchema: { type: 17 } }],
    ["orphan-then", { then: { type: 17 } }],
    ["orphan-else", { else: { type: 17 } }],
    ["rejecting-helper", { not: {} }],
    ["replacement", { required: false }],
  ];
  for (const [id, patch] of patches) {
    const input = structuredClone(fixture);
    if (patch) Object.assign(input.resources[helper], patch);
    input.document.operations.op = { input: { $ref: "#/schemas/entry" } };
    const owners = [];
    const own = (value) => {
      owners.push(value);
      return value;
    };
    try {
      const document = own(
        sdk.Document.parse(JSON.stringify(input.document)).value,
      );
      const resources = own(
        new sdk.ExternalSchemas(
          Object.entries(input.resources).map(([uri, value]) => [
            uri,
            own(sdk.parseJson(JSON.stringify(value)).value),
          ]),
        ),
      );
      const ready = prepareInput(document, "op", { resources });
      if (patch) {
        if (ready.status === "ready") {
          own(ready.schema);
          throw Error(`${id}: invalid schema was admitted`);
        }
        if (ready.failure?.kind !== "invalid-external-schema")
          throw Error(`${id}: ${JSON.stringify(ready)}`);
        results.push({ id, status: ready.failure.kind });
      } else {
        if (ready.status !== "ready") throw Error(JSON.stringify(ready));
        const schema = own(ready.schema);
        for (const [text, expected] of [
          ["1", "satisfies"],
          ["-1", "fails"],
          ["false", "fails"],
        ]) {
          const value = own(sdk.parseJson(text).value);
          for (const method of ["evaluate", "explain"]) {
            const outcome = schema[method](value);
            if (outcome.status !== expected)
              throw Error(
                `${id}/${text}/${method}: ${JSON.stringify(outcome)}`,
              );
          }
        }
        results.push({ id, status: "ready" });
      }
    } finally {
      for (const owner of owners.reverse()) owner.dispose();
    }
  }
  return results;
}
