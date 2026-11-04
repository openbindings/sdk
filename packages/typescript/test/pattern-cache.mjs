import { prepareInput } from "./conformant-contracts.mjs";

// The supplied custom profile selects the original evaluator through public APIs.
export function patternCacheCases(sdk) {
  const root = "https://cache.test/root";
  const meta = "https://cache.test/meta";
  const rows = [
    ...[100, 1_000, 10_000].map((count) => ({
      id: `empty-alternatives-${count}`,
      pattern: "|".repeat(count),
      values: [
        ["", "satisfies"],
        ["x", "satisfies"],
      ],
    })),
    {
      id: "bounded-optional",
      pattern: "^(?:a|b?){0,100}z$",
      values: [
        ["ababz", "satisfies"],
        ["abab", "fails"],
      ],
    },
    {
      id: "unicode-union",
      pattern: "^(?:é|☃|a)*x$",
      values: [
        ["☃éax", "satisfies"],
        ["☃éa", "fails"],
      ],
    },
  ];
  const results = [];
  for (const row of rows) {
    const owners = [];
    const own = (value) => {
      owners.push(value);
      return value;
    };
    try {
      const document = own(
        sdk.Document.parse(
          JSON.stringify({
            openbindings: "0.2.0",
            operations: { op: { input: { $ref: root } } },
          }),
        ).value,
      );
      const resource = own(
        sdk.parseJson(
          JSON.stringify({
            $schema: meta,
            pattern: row.pattern,
          }),
        ).value,
      );
      const profile = own(
        sdk.parseJson(
          JSON.stringify({
            $schema: "https://json-schema.org/draft/2020-12/schema",
            $vocabulary: {
              "https://json-schema.org/draft/2020-12/vocab/core": true,
              "https://json-schema.org/draft/2020-12/vocab/validation": true,
            },
            type: ["object", "boolean"],
          }),
        ).value,
      );
      const resources = own(
        new sdk.ExternalSchemas([
          [root, resource],
          [meta, profile],
        ]),
      );
      const setup = prepareInput(document, "op", { resources });
      if (setup.status !== "ready")
        throw Error(`${row.id}: ${JSON.stringify(setup)}`);
      const schema = own(setup.schema);
      for (const [text, expected] of row.values) {
        const value = own(sdk.parseJson(JSON.stringify(text)).value);
        for (const method of ["evaluate", "explain"]) {
          const outcome = schema[method](value);
          if (outcome.status !== expected)
            throw Error(`${row.id}/${method}: ${JSON.stringify(outcome)}`);
        }
      }
      results.push({ id: row.id, status: "passed" });
    } finally {
      for (const owner of owners.reverse()) owner.dispose();
    }
  }
  return results;
}
