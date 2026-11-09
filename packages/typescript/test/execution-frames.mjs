import { prepareInput } from "./conformant-contracts.mjs";

// A supported supplied custom profile uses the original-node evaluator, including
// before that engine becomes the ordinary default. No testing-only engine switch.
export function executionFrameCases(sdk) {
  const root = "https://frames.test/root";
  const meta = "https://frames.test/meta";
  const metaText = JSON.stringify({
    $schema: "https://json-schema.org/draft/2020-12/schema",
    $vocabulary: Object.fromEntries(
      ["core", "applicator", "validation", "unevaluated"].map((name) => [
        `https://json-schema.org/draft/2020-12/vocab/${name}`,
        true,
      ]),
    ),
    type: ["object", "boolean"],
  });
  const recursive = {
    $defs: {
      node: {
        anyOf: [
          { type: "integer" },
          { type: "array", items: { $ref: "#/$defs/node" } },
          { type: "object", additionalProperties: { $ref: "#/$defs/node" } },
        ],
      },
    },
    $ref: "#/$defs/node",
  };
  const rows = [];
  for (const depth of [32, 64, 256]) {
    for (const [leaf, expected] of [
      ["1", "satisfies"],
      ["false", "fails"],
    ]) {
      rows.push({
        id: `depth-${depth}-${expected}`,
        schema: recursive,
        value: "[".repeat(depth) + leaf + "]".repeat(depth),
        expected,
      });
    }
  }
  for (const [leaf, expected] of [
    ["1", "satisfies"],
    ["false", "fails"],
  ]) {
    rows.push({
      id: `alias-items-${expected}`,
      schema: {
        $defs: { item: { type: "integer" } },
        items: { $ref: "#/$defs/item" },
      },
      value: "[" + "1,".repeat(50_000) + leaf + "]",
      expected,
    });
  }
  const results = [];
  for (const row of rows) {
    const owners = [];
    try {
      const own = (value) => {
        owners.push(value);
        return value;
      };
      const document = own(
        sdk.Document.parse(
          JSON.stringify({
            openbindings: "0.2.0",
            operations: { op: { input: { $ref: root } } },
          }),
        ).value,
      );
      const resource = own(
        sdk.parseJson(JSON.stringify({ $schema: meta, ...row.schema })).value,
      );
      const metaSchema = own(sdk.parseJson(metaText).value);
      const resources = own(
        new sdk.ExternalSchemas([
          [root, resource],
          [meta, metaSchema],
        ]),
      );
      const setup = prepareInput(document, "op", { resources });
      if (setup.status !== "ready")
        throw Error(`${row.id}: ${JSON.stringify(setup)}`);
      const schema = own(setup.schema);
      const value = own(sdk.parseJson(row.value).value);
      for (const method of ["evaluate", "explain"]) {
        const result = schema[method](value);
        if (result.status !== row.expected)
          throw Error(`${row.id}/${method}: ${JSON.stringify(result)}`);
      }
      results.push({ id: row.id, status: row.expected });
    } finally {
      for (const owner of owners.reverse()) owner.dispose();
    }
  }
  return results;
}
