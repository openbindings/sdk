import { prepareInput } from "./conformant-contracts.mjs";

// A separate custom-profile entry selects the original interpreter through the
// public API. Every independently authored target schema and expected location
// remains unchanged, including Boolean schemas and exact numeric text.
export function originalExplainCases(sdk, controls, testing) {
  const root = "https://explain.invalid/root";
  const entry = "https://explain.invalid/entry";
  const meta = "https://explain.invalid/selector-meta";
  const results = [];
  // Fixed meta-schema arenas initialize lazily. Warm the complete job first,
  // then require exact cleanup on a second complete pass through every case.
  for (const measured of [false, true])
    for (const row of controls.cases) {
      const before = testing.liveStorageOwners();
      const owners = [];
      const own = (value) => {
        if (!value) throw Error(`${row.id}: missing owner`);
        owners.push(value);
        return value;
      };
      const parse = (text) => own(sdk.parseJson(text).value);
      const check = (condition, message) => {
        if (!condition) throw Error(`${row.id}: ${message}`);
      };
      try {
        const document = own(
          sdk.Document.parse(
            JSON.stringify({
              openbindings: "0.2.0",
              operations: { op: { input: { $ref: entry } } },
            }),
          ).value,
        );
        const sources = [
          [root, parse(row.schema_text ?? JSON.stringify(row.schema))],
          [entry, parse(JSON.stringify({ $schema: meta, $ref: root }))],
          [
            meta,
            parse(
              JSON.stringify({
                $schema: "https://json-schema.org/draft/2020-12/schema",
                $vocabulary: {
                  "https://json-schema.org/draft/2020-12/vocab/core": true,
                },
                type: ["object", "boolean"],
              }),
            ),
          ],
          ...Object.entries(row.resources ?? {}).map(([uri, schema]) => [
            uri,
            parse(JSON.stringify(schema)),
          ]),
        ];
        const resources = own(new sdk.ExternalSchemas(sources));
        const prepared = prepareInput(document, "op", { resources });
        check(prepared.status === "ready", JSON.stringify(prepared));
        const schema = own(prepared.schema);
        let value = parse(row.value_text ?? JSON.stringify(row.value));
        if (row.evaluate_subtree !== undefined)
          value = own(value.at(row.evaluate_subtree));
        const outcome = schema.evaluate(value);
        const explained = schema.explain(value);
        check(outcome.status === row.required_outcome, JSON.stringify(outcome));
        check(
          explained.status === row.required_outcome,
          JSON.stringify(explained),
        );
        if (outcome.status === "fails")
          check(outcome.problems.length === 0, "Outcome created problems");
        if (explained.status === "fails") {
          check(
            explained.problemsComplete === row.problems_complete,
            "diagnostic completeness",
          );
          check(
            explained.problems.length ===
              Number(row.must_report_problem_when_funded),
            "unexpected problem count",
          );
          if (row.must_report_problem_when_funded)
            check(explained.problems.length === 1, "missing funded witness");
          for (const problem of explained.problems) {
            check(
              !row.forbidden_keywords.includes(problem.code),
              "irrelevant branch",
            );
            if (row.keyword !== undefined)
              check(
                problem.code ===
                  (row.keyword === "$false" ? "falseSchema" : row.keyword),
                "wrong failure keyword",
              );
            if (row.instance_pointer !== undefined)
              check(
                problem.instance === row.instance_pointer,
                "instance pointer",
              );
            if (row.schema_pointer !== undefined)
              check(
                problem.schema.pointer === row.schema_pointer,
                "schema pointer",
              );
            check(
              problem.schema.sourceUri === (row.schema_source_uri ?? root),
              "source URI",
            );
            check(
              value.locate(problem.instance) !== undefined,
              "instance provenance",
            );
            check(
              resources.locate(problem.schema) !== undefined,
              "schema provenance",
            );
            check(
              problem.message === "value fails this schema assertion",
              "fixed message",
            );
          }
        }
        if (measured) results.push({ id: row.id, status: explained.status });
      } finally {
        for (const owner of owners.reverse()) owner.dispose();
      }
      if (measured)
        check(testing.liveStorageOwners() === before, "retained storage owner");
    }
  return results;
}
