import test from "node:test";
import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { conformantContracts } from "./conformant-contracts.mjs";
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
function subset(expected, actual, id) {
  if (expected && typeof expected === "object") {
    for (const [key, value] of Object.entries(expected))
      subset(value, actual?.[key], id);
  } else assert.deepEqual(actual, expected, id);
}
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
    const values = (c.resources ?? []).map((r) => [
      r.uri,
      sdk.parseJson(JSON.stringify(r.schema)).value,
    ]);
    const resources = new sdk.SchemaResources(values);
    let context, contract;
    try {
      const limits = Object.fromEntries(
        Object.entries(c.limits ?? {}).map(([key, value]) => [
          key.replace(/_([a-z])/g, (_, c) => c.toUpperCase()),
          value,
        ]),
      );
      context = conformantContracts(parsed.value, { resources, limits });
      const setup = context.prepare("op", "input");
      contract = setup.valueContract;
      const result =
        setup.status === "ready"
          ? contract.validate(c.value)
          : { outcome: "no-verdict", detail: setup.detail };
      const actual = {
        stage: setup.status === "ready" ? "ready" : "preparation",
        outcome: {
          satisfies: "S",
          fails: "F",
          undefined: "undefined",
          "no-verdict": "N",
        }[result.outcome],
      };
      if (result.detail)
        actual.detail = {
          reason: result.detail.reason,
          code: result.detail.code,
          location: result.detail.location
            ? {
                resource: result.detail.location.resource ?? null,
                pointer: result.detail.location.pointer,
              }
            : null,
        };
      const expected = structuredClone(c.expected);
      if (expected.outcome === "N" && expected.detail?.reason === "undefined") {
        expected.outcome = "undefined";
        delete expected.detail.reason;
      }
      if (result.outcome === "undefined") {
        assert.equal("reason" in result.detail, false, c.id);
        assert.equal(typeof result.detail.message, "string", c.id);
      }
      subset(expected, actual, c.id);
      receipt.push({ id: c.id, actual });
    } finally {
      contract?.dispose();
      context?.dispose();
      resources.dispose();
      for (const [, value] of values) value.dispose();
      parsed.value.dispose();
    }
  }
  if (process.env.REFERENCE_RECEIPT)
    await writeFile(process.env.REFERENCE_RECEIPT, JSON.stringify(receipt));
});
