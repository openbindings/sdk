import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
await sdk.initialize();

const open = (text) => {
  const parsed = sdk.Document.parse(text);
  assert.equal(parsed.status, "parsed");
  return parsed.value;
};

test("reference inspection refuses malformed options on drafts and proofs", () => {
  const draft = open('{"openbindings":"0.2.0","operations":{}}');
  const validation = draft.validate();
  assert.equal(validation.status, "conformant");
  const resources = new sdk.ExternalSchemas();
  try {
    for (const document of [draft, validation.document]) {
      for (const options of [
        null,
        false,
        1,
        "options",
        [],
        { extra: true },
        { signals: new AbortController().signal },
        { signal: { aborted: true } },
        { signal: null },
        { resources: null },
        { resources: {} },
        { resources: draft },
      ]) {
        assert.throws(
          () => document.references(options),
          (e) => e instanceof sdk.SdkError && e.code === "invalid-work-options",
        );
      }
      assert.equal(document.references({ resources }).complete, true);
      assert.equal(
        document.references({ signal: new AbortController().signal }).complete,
        true,
      );
      assert.equal(
        document.references({ signal: AbortSignal.abort() }).complete,
        false,
      );
    }
  } finally {
    resources.dispose();
    validation.document.dispose();
    draft.dispose();
  }
});

test("missing and malformed versions inspect every reference under 0.2", () => {
  const body = {
    operations: {
      op: {
        input: {
          allOf: [
            { $ref: "#/schemas/target" },
            { $dynamicRef: "#target" },
            { $ref: "#/schemas/missing" },
            { $ref: "https://example.test/external" },
          ],
        },
      },
    },
    schemas: { target: { $dynamicAnchor: "target" } },
    sources: { s: { kind: "k", data: { $ref: "opaque" } } },
  };
  const external = sdk.ExactJson.from({
    $defs: { target: true },
    $ref: "#/$defs/target",
  });
  const resources = new sdk.ExternalSchemas([
    ["https://example.test/external", external],
  ]);
  external.dispose();
  const valid = open(JSON.stringify({ openbindings: "0.2.0", ...body }));
  try {
    const expected = valid.references();
    const supplied = valid.references({ resources });
    assert.equal(expected.complete, true);
    assert.equal(supplied.complete, true);
    assert.equal(expected.limitation, null);
    assert.equal(supplied.limitation, null);
    assert.equal(expected.references.length, 4);
    assert.equal(supplied.references.length, 5);
    assert.deepEqual(supplied.references[0].resolution, {
      status: "located",
      target: { sourceUri: null, pointer: "/schemas/target" },
      dynamicLookup: false,
    });
    assert.deepEqual(supplied.references[1].resolution, {
      status: "located",
      target: { sourceUri: null, pointer: "/schemas/target" },
      dynamicLookup: true,
    });
    assert.equal(supplied.references[2].resolution.status, "unresolved");
    assert.equal(
      supplied.references[2].resolution.detail.reason,
      "missing-target",
    );
    assert.deepEqual(supplied.references[4].resolution, {
      status: "located",
      target: {
        sourceUri: "https://example.test/external",
        pointer: "/$defs/target",
      },
      dynamicLookup: false,
    });
    for (const openbindings of [undefined, 1, 5, null, "bad"]) {
      const text = JSON.stringify({ openbindings, ...body });
      const draft = open(text);
      try {
        assert.deepEqual(draft.references(), expected);
        assert.deepEqual(draft.references({ resources }), supplied);
        assert.equal(
          draft.references({ resources, signal: AbortSignal.abort() })
            .limitation.reason,
          "cancelled",
        );
        assert.deepEqual(draft.toBytes(), new TextEncoder().encode(text));
      } finally {
        draft.dispose();
      }
    }
  } finally {
    resources.dispose();
    valid.dispose();
  }
});

test("unsupported versions and duplicate members still refuse inspection", () => {
  for (const [text, code, reason] of [
    [
      '{"openbindings":"0.3.0","operations":{"op":{"input":{"$ref":"#/schemas/missing"}}}}',
      "unsupported-version",
      "unsupported-capability",
    ],
    [
      '{"openbindings":"0.2.0","operations":{},"x":1,"x":2}',
      "duplicate-members",
      "resolver-failure",
    ],
    [
      '{"openbindings":5,"openbindings":"0.2.0","operations":{}}',
      "duplicate-members",
      "resolver-failure",
    ],
  ]) {
    const draft = open(text);
    try {
      const report = draft.references();
      assert.equal(report.complete, false);
      assert.deepEqual(report.references, []);
      assert.equal(report.limitation.code, code);
      assert.equal(report.limitation.reason, reason);
      assert.equal(report.limitation.location, null);
    } finally {
      draft.dispose();
    }
  }
});
