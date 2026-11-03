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
const directory = new URL(
  "../../../crates/openbindings-json-schema-evaluator/tests/fixtures/",
  import.meta.url,
);
for (const [name, count] of [
  ["current-reference-obligations.json", 82],
  ["current-auxiliary-obligations.json", 50],
  ["semantic-soundness.json", 26],
  ["semantic-recursive-summaries.json", 24],
  ["semantic-recursive-extension.json", 33],
  ["semantic-coordinate-refinement.json", 71],
  ["semantic-limits.json", 16],
]) {
  test(`current semantic obligations through Wasm: ${name}`, async () => {
    const loaded = JSON.parse(await readFile(new URL(name, directory), "utf8"));
    const cases = loaded.cases ?? loaded;
    assert.equal(cases.length, count);
    for (const c of cases) {
      const parsed = sdk.Document.parse(
        JSON.stringify(
          c.document ?? {
            openbindings: "0.2.0",
            operations: { op: { input: c.schema } },
          },
        ),
      );
      assert.equal(parsed.status, "parsed", c.id);
      const doc = parsed.value;
      let resources, prepared, value;
      const supplied = [];
      try {
        const validation = doc.validate();
        if (c.required.startsWith("admission:")) {
          assert.equal(validation.status, "report", c.id);
          assert.equal(
            validation.report.evidence[c.required.slice(10)],
            "violated",
            c.id,
          );
          continue;
        }
        assert.equal(validation.status, "conformant", c.id);
        validation.document.dispose();
        for (const resource of c.resources ?? []) {
          const parsed = sdk.parseJson(
            resource.schema_text ?? JSON.stringify(resource.schema),
          );
          supplied.push([resource.uri, parsed.value]);
        }
        resources = new sdk.ExternalSchemas(supplied);
        const limits = Object.fromEntries(
          Object.entries(c.limits ?? {}).map(([key, value]) => [
            key.replace(/_([a-z])/g, (_, x) => x.toUpperCase()),
            value,
          ]),
        );
        const setup = prepareInput(doc, "op", { resources, limits });
        if (setup.status === "refused") {
          assert.equal(
            c.required,
            "limit",
            `${c.id}: ${JSON.stringify(setup)}`,
          );
          assert.equal(setup.failure.kind, "limit", c.id);
          continue;
        }
        assert.equal(setup.status, "ready", c.id);
        prepared = setup.schema;
        value = sdk.parseJson(
          c.valueText ?? c.value_text ?? JSON.stringify(c.value),
        ).value;
        for (const result of [
          prepared.evaluate(value),
          prepared.explain(value),
        ]) {
          const actual =
            result.status === "satisfies"
              ? "S"
              : result.status === "fails"
                ? "F"
                : result.status === "undefined"
                  ? "U"
                  : result.status === "no-verdict" &&
                      result.detail.reason === "resource-unavailable"
                    ? "external"
                    : result.status === "no-verdict" &&
                        result.detail.reason === "limit-exceeded"
                      ? "limit"
                      : "N";
          assert.equal(
            actual,
            c.required === "N:resource-unavailable" ? "external" : c.required,
            `${c.id}: ${JSON.stringify(result)}`,
          );
        }
      } finally {
        value?.dispose();
        prepared?.dispose();
        resources?.dispose();
        for (const [, value] of supplied) value.dispose();
        doc.dispose();
      }
    }
  });
}
