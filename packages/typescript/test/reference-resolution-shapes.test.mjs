import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
await sdk.initialize();
const expected = JSON.parse(
  (
    await readFile(
      new URL(
        "../../../crates/openbindings/tests/fixtures/reference_resolution_shapes.json",
        import.meta.url,
      ),
      "utf8",
    )
  )
    .replace(/"source_uri":/g, '"sourceUri":')
    .replace(/"resource_root":/g, '"resourceRoot":')
    .replace(/"resource_uri":/g, '"resourceUri":')
    .replace(/"dynamic_lookup":/g, '"dynamicLookup":'),
);
test("all original resolution snapshots agree with Rust including resource roots", () => {
  for (const shape of [
    "fragment",
    "identical",
    "long-leaf",
    "long-directory",
    "deep-relative-id",
  ]) {
    const base = `https://e.test/${"a".repeat(32)}${shape === "long-directory" ? "/" : ""}`;
    const entry = {
      $id: base,
      $defs: { s0: true, s1: false, s2: { type: "string" } },
    };
    if (shape === "deep-relative-id") {
      let nested = { allOf: [{ $ref: "#" }, { $ref: "s0" }] };
      for (let i = 0; i < 8; i++) nested = { $id: "a/", not: nested };
      entry.not = nested;
    } else
      entry.allOf = Array.from({ length: 3 }, (_, i) => ({
        $ref:
          shape === "fragment"
            ? `#/$defs/s${i}`
            : shape === "identical"
              ? "#"
              : `s${i}`,
      }));
    const document = sdk.Document.parse(
      JSON.stringify({
        openbindings: "0.2.0",
        operations: {},
        schemas: { entry },
      }),
    ).value;
    const validation = document.validate();
    assert.equal(validation.status, "conformant");
    const proof = validation.document;
    try {
      const directory =
        shape === "long-directory"
          ? base
          : shape === "deep-relative-id"
            ? `https://e.test/${"a/".repeat(8)}`
            : "https://e.test/";
      for (const supplied of [false, true]) {
        const external = sdk.ExactJson.from({
          $defs: { target: true },
          $ref: "#/$defs/target",
        });
        const resources = new sdk.ExternalSchemas(
          Array.from({ length: supplied ? 3 : 0 }, (_, i) => [
            `${directory}s${i}`,
            external,
          ]),
        );
        external.dispose();
        try {
          for (const owner of [document, proof])
            assert.deepEqual(
              owner.references({ resources }),
              expected[`${shape}/${supplied}`],
            );
        } finally {
          resources.dispose();
        }
      }
    } finally {
      proof.dispose();
      document.dispose();
    }
  }
});
