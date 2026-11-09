import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
await sdk.initialize();
const cases = JSON.parse(
  await readFile(
    new URL(
      "../../../crates/openbindings/tests/fixtures/reference_resource_roots.json",
      import.meta.url,
    ),
    "utf8",
  ),
);
const camel = (value) =>
  JSON.parse(
    JSON.stringify(value)
      .replace(/"source_uri":/g, '"sourceUri":')
      .replace(/"resource_root":/g, '"resourceRoot":'),
  );
const ordered = (rows) =>
  rows.sort((a, b) =>
    JSON.stringify(a.location).localeCompare(JSON.stringify(b.location)),
  );
for (const fixture of cases) {
  test(`resource roots agree with Rust: ${fixture.name}`, () => {
    const values = fixture.resources.map((r) => sdk.ExactJson.from(r.document));
    const resources = new sdk.ExternalSchemas(
      fixture.resources.map((r, i) => [r.uri, values[i]]),
    );
    values.forEach((v) => v.dispose());
    let expected;
    try {
      for (const version of [undefined, "0.2.0"]) {
        const document = sdk.Document.parse(
          JSON.stringify({ ...fixture.document, openbindings: version }),
        ).value;
        let proof;
        try {
          const report = document.references({ resources });
          assert.equal(report.complete, true);
          assert.deepEqual(
            ordered(
              report.references.map((r) => ({
                location: r.location,
                resourceRoot: r.resourceRoot,
                status: r.resolution.status,
              })),
            ),
            ordered(camel(fixture.expected)),
          );
          if (expected) assert.deepEqual(report, expected);
          expected = report;
          if (version && fixture.conformant) {
            const validation = document.validate();
            assert.equal(validation.status, "conformant");
            proof = validation.document;
            assert.deepEqual(proof.references({ resources }), report);
          }
        } finally {
          proof?.dispose();
          document.dispose();
        }
      }
    } finally {
      resources.dispose();
    }
  });
}
