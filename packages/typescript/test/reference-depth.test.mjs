import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
await sdk.initialize();

test("inspection reports present and missing targets at validation's depth", () => {
  for (const depth of [300, 2000]) {
    for (const present of [false, true]) {
      const name = present ? "target" : "missing";
      const text = `{"openbindings":"0.2.0","operations":{},"schemas":{"target":true,"entry":${'{"not":'.repeat(depth)}{"$ref":"#/schemas/${name}"}${"}".repeat(depth)}}}`;
      const parsed = sdk.Document.parse(text);
      assert.equal(parsed.status, "parsed");
      const document = parsed.value;
      try {
        const report = document.references();
        assert.equal(report.complete, true);
        assert.equal(report.limitation, null);
        assert.equal(report.references.length, 1);
        const reference = report.references[0];
        const holder = `/schemas/entry${"/not".repeat(depth)}`;
        assert.deepEqual(reference.location, {
          sourceUri: null,
          pointer: `${holder}/$ref`,
        });
        if (present) {
          assert.deepEqual(reference.resolution, {
            status: "located",
            target: { sourceUri: null, pointer: "/schemas/target" },
            dynamicLookup: false,
          });
        } else {
          assert.equal(reference.resolution.status, "unresolved");
          assert.equal(reference.resolution.detail.reason, "missing-target");
          assert.equal(reference.resolution.detail.code, "missing-target");
          assert.deepEqual(reference.resolution.detail.location, {
            sourceUri: null,
            pointer: holder,
          });
        }
      } finally {
        document.dispose();
      }
    }
  }
});

test("deep relative identities refuse cleanly at the identity allowance", () => {
  for (const depth of [2000, 2360]) {
    const text = `{"openbindings":"0.2.0","operations":{},"schemas":{"entry":{"$id":"https://e.test/","not":${'{"$id":"a/","not":'.repeat(depth)}{"$ref":"#"}${"}".repeat(depth)}}}}`;
    const parsed = sdk.Document.parse(text);
    assert.equal(parsed.status, "parsed");
    const document = parsed.value;
    try {
      const report = document.references();
      if (depth === 2000) {
        assert.equal(report.complete, true);
        assert.equal(report.limitation, null);
        assert.equal(report.references.length, 1);
        assert.equal(report.references[0].resolution.status, "located");
        assert.equal(
          report.references[0].resolution.target.pointer,
          `/schemas/entry${"/not".repeat(depth)}`,
        );
      } else {
        assert.equal(report.complete, false);
        assert.deepEqual(report.references, []);
        assert.equal(report.limitation.reason, "limit-exceeded");
        assert.equal(report.limitation.code, "schema-identity-bytes");
        assert.equal(report.limitation.location, null);
      }
    } finally {
      document.dispose();
    }
  }
});
