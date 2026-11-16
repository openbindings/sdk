import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
await sdk.initialize();

test("inherited options use the same SDK error category as malformed own options", () => {
  const text = '{"openbindings":"0.2.0","operations":{"op":{"input":true}}}';
  const document = sdk.Document.parse(text).value;
  const proof = document.validate().document;
  const schema = proof.inputSchema("op");
  const prepared = schema.prepare().schema;
  const value = sdk.parseJson("true").value;
  const resources = new sdk.ExternalSchemas([["urn:options", value]]);
  try {
    const cases = [
      [
        "invalid-json-limits",
        "limits",
        {},
        7,
        [
          (options) => sdk.parseJson("7", options),
          (options) => sdk.Document.parse(text, options),
          (options) => sdk.validateDocument(text, options),
        ],
      ],
      [
        "invalid-json-limits",
        "maxBytes",
        0,
        -1,
        [
          (limits) => sdk.parseJson("7", { limits }),
          (limits) => sdk.Document.parse(text, { limits }),
          (limits) => sdk.validateDocument(text, { limits }),
        ],
      ],
      [
        "invalid-validation-options",
        "unknown",
        0,
        0,
        [(options) => document.validate(options)],
      ],
      [
        "invalid-validation-limits",
        "maxFindings",
        0,
        -1,
        [(limits) => document.validate({ limits })],
      ],
      [
        "invalid-reference-limits",
        "maxReportBytes",
        0,
        -1,
        [(limits) => document.references({ limits })],
      ],
      [
        "invalid-preparation-limits",
        "maxSdkReservationBytes",
        0,
        -1,
        [
          (preparationLimits) => schema.prepare({ preparationLimits }),
          (preparationLimits) =>
            schema.prepare({ preparationLimits, resources }),
          (preparationLimits) =>
            resources.prepare("urn:options", { preparationLimits }),
          (limits) => new sdk.ExternalSchemas([], { limits }),
          (limits) => resources.with("urn:extra", value, { limits }),
        ],
      ],
      [
        "invalid-evaluator-limits",
        "patternDepth",
        0,
        -1,
        [
          (limits) => schema.prepare({ limits }),
          (limits) => resources.prepare("urn:options", { limits }),
        ],
      ],
      [
        "invalid-work-options",
        "signal",
        AbortSignal.abort(),
        7,
        [
          (options) => schema.prepare(options),
          (options) => resources.prepare("urn:options", options),
          (options) => new sdk.ExternalSchemas([], options),
          (options) => resources.with("urn:extra", value, options),
          (options) => document.references(options),
          (options) => prepared.evaluate(value, options),
          (options) => prepared.evaluateJson("7", options),
          (options) => prepared.explainJson("7", options),
          (options) => prepared.explain(value, options),
        ],
      ],
    ];
    for (const [code, key, inherited, malformed, calls] of cases)
      for (const call of calls)
        for (const options of [
          Object.create({ [key]: inherited }),
          { [key]: malformed },
        ])
          assert.throws(call.bind(null, options), (error) => {
            assert.ok(error instanceof sdk.SdkError);
            assert.equal(error.code, code);
            return true;
          });
    const ready = schema.prepare({
      preparationLimits: Object.assign(Object.create(null), {
        maxSchemaNodes: 1000,
      }),
    });
    assert.equal(ready.status, "ready");
    ready.schema.dispose();
  } finally {
    for (const owner of [resources, value, prepared, schema, proof, document])
      owner.dispose();
  }
});
