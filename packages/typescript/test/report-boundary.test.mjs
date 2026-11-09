import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
import { decode } from "../dist/diagnostics.js";
await sdk.initialize();

test("report decoding preserves ordered fields, nulls and mutable own properties", () => {
  const decoded = decode(
    '{"source_uri":null,"byte_offset":7,"nested":[{"dynamic_lookup":false,"resource_uri":null}],"constructor":"data","has_own_property":true,"a_b":1,"aB":2}',
  );
  assert.equal(
    JSON.stringify(decoded),
    '{"sourceUri":null,"byteOffset":7,"nested":[{"dynamicLookup":false,"resourceUri":null}],"constructor":"data","hasOwnProperty":true,"aB":2}',
  );
  assert.equal(Object.getPrototypeOf(decoded), Object.prototype);
  assert.equal(Object.hasOwn(decoded, "absent"), false);
  assert.deepEqual(Object.getOwnPropertyDescriptor(decoded, "sourceUri"), {
    value: null,
    writable: true,
    enumerable: true,
    configurable: true,
  });
  decoded.nested[0].resourceUri = "changed";
  delete decoded.byteOffset;
  assert.equal(decoded.nested[0].resourceUri, "changed");
  assert.equal(Object.hasOwn(decoded, "byteOffset"), false);
});

test("repeated reference origins stay detached and independently mutable", () => {
  const name = 'a/~"\n😀\ud800x\udc00';
  const pointer = `/schemas/${name.replace(/~/g, "~0").replace(/\//g, "~1")}`;
  const document = sdk.Document.parse(
    JSON.stringify({
      schemas: {
        [name]: {
          $id: "https://example.test/resource",
          allOf: [{ $ref: "#" }, { $ref: "#" }],
        },
      },
    }),
  ).value;
  let report, again;
  try {
    report = document.references();
    again = document.references();
  } finally {
    document.dispose();
  }
  assert.equal(report.complete, true);
  assert.equal(report.limitation, null);
  assert.deepEqual(Object.keys(report), [
    "references",
    "complete",
    "limitation",
  ]);
  for (const [i, reference] of report.references.entries()) {
    assert.deepEqual(Object.keys(reference), [
      "location",
      "resourceRoot",
      "keyword",
      "spelling",
      "resolution",
    ]);
    assert.deepEqual(reference.location, {
      sourceUri: null,
      pointer: `${pointer}/allOf/${i}/$ref`,
    });
    assert.deepEqual(reference.resourceRoot, { sourceUri: null, pointer });
    assert.deepEqual(reference.resolution, {
      status: "located",
      target: { sourceUri: null, pointer },
      dynamicLookup: false,
    });
  }
  report.references[0].resourceRoot.pointer = "edited";
  assert.equal(report.references[0].resolution.target.pointer, pointer);
  report.references[0].location.sourceUri = "edited";
  report.references[0].resolution.target.pointer = "target edited";
  assert.equal(report.references[0].resourceRoot.pointer, "edited");
  assert.equal(report.references[1].resourceRoot.pointer, pointer);
  assert.equal(report.references[1].location.sourceUri, null);
  assert.equal(report.references[1].resolution.target.pointer, pointer);
  assert.equal(again.references[0].resourceRoot.pointer, pointer);
  assert.equal(again.references[0].resolution.target.pointer, pointer);
});
