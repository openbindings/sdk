import { prepareInput } from "./conformant-contracts.mjs";

// This supported custom profile selects original-node evaluation through the
// public API while the outgoing compiler remains the ordinary default.
export function referencePreparationCases(sdk) {
  const root = "https://reference-preparation.test/root";
  const meta = "https://reference-preparation.test/meta";
  const metaText = JSON.stringify({
    $schema: "https://json-schema.org/draft/2020-12/schema",
    $vocabulary: Object.fromEntries(
      ["core", "applicator", "validation"].map((name) => [
        `https://json-schema.org/draft/2020-12/vocab/${name}`,
        true,
      ]),
    ),
    type: ["object", "boolean"],
  });
  const results = [];
  for (const [count, length] of [
    [1000, 256],
    [5000, 1024],
  ]) {
    const owners = [];
    const own = (value) => {
      owners.push(value);
      return value;
    };
    try {
      const uri = "https://memory.test/" + "a".repeat(length);
      const document = own(
        sdk.Document.parse(
          JSON.stringify({
            openbindings: "0.2.0",
            operations: { op: { input: { $ref: root } } },
          }),
        ).value,
      );
      const source = own(
        sdk.parseJson(
          JSON.stringify({
            $schema: meta,
            allOf: Array.from({ length: count }, () => ({
              $ref: uri + "#/$defs/value",
            })),
          }),
        ).value,
      );
      const target = own(
        sdk.parseJson(
          JSON.stringify({
            $id: uri,
            $defs: { value: { type: "integer" } },
          }),
        ).value,
      );
      const metaSchema = own(sdk.parseJson(metaText).value);
      const resources = own(
        new sdk.ExternalSchemas([
          [root, source],
          [uri, target],
          [meta, metaSchema],
        ]),
      );
      const ready = prepareInput(document, "op", { resources });
      if (ready.status !== "ready")
        throw Error(`${count}: ${JSON.stringify(ready)}`);
      const schema = own(ready.schema);
      for (const [text, expected] of [
        ["1", "satisfies"],
        ['"x"', "fails"],
      ]) {
        const value = own(sdk.parseJson(text).value);
        for (const method of ["evaluate", "explain"]) {
          const result = schema[method](value);
          if (result.status !== expected)
            throw Error(`${count}/${method}: ${JSON.stringify(result)}`);
        }
        results.push({
          id: `references-${count}-${expected}`,
          status: expected,
        });
      }
    } finally {
      for (const owner of owners.reverse()) owner.dispose();
    }
  }
  return results;
}
