import test from "node:test";
import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { prepareInput } from "./conformant-contracts.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const input = new URL(
  "../../../crates/openbindings-json-schema-evaluator/tests/fixtures/",
  import.meta.url,
);
const cases = (
  await Promise.all(
    ["reference-matrix.json", "reference-completions.json"].map(async (name) =>
      JSON.parse(await readFile(new URL(name, input), "utf8")),
    ),
  )
).flat();
const admission = JSON.parse(
  await readFile(new URL("reference-admission.json", input), "utf8"),
);
const oracle = new Map(
  JSON.parse(
    await readFile(new URL("reference-current-oracle.json", input), "utf8"),
  ).cases.map((c) => [c.id, c]),
);
assert.equal(oracle.size, 2348);
const negatives = new Map(admission.cases.map((c) => [c.id, c]));
assert.equal(negatives.size, 194);
test("static reference matrix and concrete completions through Wasm", async () => {
  const receipt = [];
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
    const assessed = parsed.value.validate();
    if (assessed.status !== "conformant") {
      try {
        const negative = negatives.get(c.id);
        assert.ok(negative, "Unexpected admission refusal: " + c.id);
        assert.deepEqual(c, negative.historical_case, c.id);
        assert.equal(assessed.status, "report", c.id);
        assert.equal(assessed.report.conclusion, "non-conformant", c.id);
        for (const rule of negative.required_violated_rules)
          assert.equal(assessed.report.evidence[rule], "violated", c.id);
        receipt.push({
          id: c.id,
          actual: {
            stage: "admission",
            outcome: "non-conformant",
            evidence: assessed.report.evidence,
          },
        });
      } finally {
        parsed.value.dispose();
      }
      continue;
    }
    assessed.document.dispose();
    assert.equal(
      negatives.has(c.id),
      false,
      "Expected admission refusal: " + c.id,
    );
    const values = (c.resources ?? []).map((r) => [
      r.uri,
      sdk.parseJson(JSON.stringify(r.schema)).value,
    ]);
    const resources = new sdk.ExternalSchemas(values);
    let contract;
    try {
      const limits = Object.fromEntries(
        Object.entries(c.limits ?? {}).map(([key, value]) => [
          key.replace(/_([a-z])/g, (_, c) => c.toUpperCase()),
          value,
        ]),
      );
      const setup = prepareInput(parsed.value, "op", { resources, limits });
      contract = setup.schema;
      const result =
        setup.status === "ready"
          ? contract.evaluate(c.value)
          : { status: "refused", failure: setup.failure };
      const actual = {
        stage: setup.status === "ready" ? "ready" : "preparation",
        outcome: {
          satisfies: "S",
          fails: "F",
          undefined: "undefined",
          "no-verdict": "N",
          refused: "refused",
        }[result.status],
      };
      if (result.failure) actual.failure = result.failure;
      if (result.detail)
        actual.detail = {
          reason: result.detail.reason,
          code: result.detail.code,
          location: result.detail.location
            ? {
                resource: result.detail.location.sourceUri ?? null,
                pointer: result.detail.location.pointer,
              }
            : null,
        };
      const required = oracle.get(c.id).required;
      if (result.status === "undefined") {
        assert.equal("reason" in result.detail, false, c.id);
        assert.equal(typeof result.detail.message, "string", c.id);
      }
      const matched =
        required === "S" || required === "F"
          ? actual.stage === "ready" && actual.outcome === required
          : required === "U"
            ? actual.stage === "ready" && actual.outcome === "undefined"
            : required === "external"
              ? actual.outcome === "N" &&
                actual.detail.reason === "resource-unavailable"
              : required === "limit"
                ? (actual.outcome === "N" &&
                    actual.detail.reason === "limit-exceeded") ||
                  (actual.outcome === "refused" &&
                    actual.failure.kind === "limit")
                : false;
      assert.ok(
        matched,
        `${c.id}: required ${required}, observed ${JSON.stringify(actual)}`,
      );
      receipt.push({ id: c.id, actual });
    } finally {
      contract?.dispose();
      resources.dispose();
      for (const [, value] of values) value.dispose();
      parsed.value.dispose();
    }
  }
  if (process.env.REFERENCE_RECEIPT)
    await writeFile(process.env.REFERENCE_RECEIPT, JSON.stringify(receipt));
});
