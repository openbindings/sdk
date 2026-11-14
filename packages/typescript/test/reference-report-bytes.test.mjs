import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);

for (const longTarget of [false, true]) {
  test(`reference report allowance bounds the long ${longTarget ? "target" : "source"} on both public routes`, () => {
    const fixedSource = "/schemas/refs";
    const fixedTarget = "/schemas/target";
    const overhead = longTarget
      ? `${fixedSource}/allOf/0/$ref`.length + "/schemas/".length
      : "/schemas//allOf/0/$ref".length + fixedTarget.length;
    const key = "a".repeat(
      1024 * 1024 - overhead - "$ref".length - "#target".length,
    );
    const source = longTarget ? fixedSource : `/schemas/${key}`;
    const target = longTarget ? `/schemas/${key}` : fixedTarget;
    const document = sdk.Document.parse(
      JSON.stringify({
        openbindings: "0.2.0",
        operations: {},
        schemas: {
          [longTarget ? "refs" : key]: {
            allOf: Array.from({ length: 20_000 }, () => ({ $ref: "#target" })),
          },
          [longTarget ? key : "target"]: { $anchor: "target" },
        },
      }),
    ).value;
    let proof;
    try {
      const validation = document.validate();
      assert.equal(validation.status, "conformant");
      proof = validation.document;
      for (const owner of [document, proof]) {
        const report = owner.references();
        assert.equal(report.complete, false);
        assert.equal(report.limitation.reason, "limit-exceeded");
        assert.equal(report.limitation.code, "reference-report-bytes");
        assert.equal(report.limitation.location, null);
        assert.equal(report.retained.length, 8);
        let bytes = 0;
        for (const [i, reference] of report.retained.entries()) {
          assert.equal(reference.location.pointer, `${source}/allOf/${i}/$ref`);
          assert.equal(reference.resolution.status, "located");
          assert.equal(reference.resolution.target.pointer, target);
          bytes +=
            reference.location.pointer.length +
            target.length +
            reference.keyword.length +
            reference.spelling.length;
        }
        assert.equal(bytes, 8 * 1024 * 1024);
        assert.ok(
          bytes + `${source}/allOf/8/$ref`.length + target.length + 11 >
            8 * 1024 * 1024,
        );
      }
    } finally {
      proof?.dispose();
      document.dispose();
    }
  });
}

const utf8 = (s) => Buffer.byteLength(s ?? "", "utf8");
const originBytes = (o) =>
  o
    ? utf8(o.sourceUri) +
      (o.pointer.isWellFormed() ? utf8(o.pointer) : 2 * o.pointer.length)
    : 0;
const entryBytes = (r) =>
  originBytes(r.location) +
  originBytes(r.resourceRoot) +
  utf8(r.keyword) +
  utf8(r.spelling) +
  (r.resolution.status === "located"
    ? originBytes(r.resolution.target)
    : originBytes(r.resolution.detail.location) +
      utf8(r.resolution.detail.resourceUri) +
      utf8(r.resolution.detail.code) +
      utf8(r.resolution.detail.message));
function bounded(report, count) {
  assert.equal(report.complete, false);
  assert.equal(report.limitation.reason, "limit-exceeded");
  assert.equal(report.limitation.code, "reference-report-bytes");
  assert.equal(report.limitation.location, null);
  assert.equal(report.limitation.resourceUri, null);
  assert.equal(report.retained.length, count);
  const bytes = report.retained.reduce((sum, r) => sum + entryBytes(r), 0);
  assert.ok(bytes <= 8 * 1024 * 1024);
  assert.ok(bytes + entryBytes(report.retained.at(-1)) > 8 * 1024 * 1024);
}

test("resource-root pointers share the allowance including unpaired names", () => {
  for (const unpaired of [false, true]) {
    const key = "a".repeat(256 * 1024) + (unpaired ? "\ud800" : "");
    const document = sdk.Document.parse(
      JSON.stringify({
        operations: {},
        schemas: {
          [key]: {
            $id: "https://id.test/root",
            allOf: Array.from({ length: 64 }, () => ({ $ref: "#" })),
          },
        },
      }),
    ).value;
    try {
      const report = document.references();
      bounded(report, unpaired ? 5 : 10);
      for (const r of report.retained) {
        assert.deepEqual(r.resourceRoot, r.resolution.target);
        assert.equal(r.resourceRoot.pointer, `/schemas/${key}`);
      }
    } finally {
      document.dispose();
    }
  }
});
