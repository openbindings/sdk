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
  test(`reference pointer allowance bounds the long ${longTarget ? "target" : "source"} on both public routes`, () => {
    const fixedSource = "/schemas/refs";
    const fixedTarget = "/schemas/target";
    const overhead = longTarget
      ? `${fixedSource}/allOf/0/$ref`.length + "/schemas/".length
      : "/schemas//allOf/0/$ref".length + fixedTarget.length;
    const key = "a".repeat(1024 * 1024 - overhead);
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
        assert.equal(report.limitation.code, "reference-pointer-bytes");
        assert.equal(report.limitation.location, null);
        assert.equal(report.references.length, 8);
        let bytes = 0;
        for (const [i, reference] of report.references.entries()) {
          assert.equal(reference.location.pointer, `${source}/allOf/${i}/$ref`);
          assert.equal(reference.resolution.status, "located");
          assert.equal(reference.resolution.target.pointer, target);
          bytes += reference.location.pointer.length + target.length;
        }
        assert.equal(bytes, 8 * 1024 * 1024);
        assert.ok(
          bytes + `${source}/allOf/8/$ref`.length + target.length >
            8 * 1024 * 1024,
        );
      }
    } finally {
      proof?.dispose();
      document.dispose();
    }
  });
}
