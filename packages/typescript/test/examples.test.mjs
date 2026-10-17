import { liveStorageOwners } from "../dist/testing.js";
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { initialize } from "../dist/index.js";
import { exampleDocument, firstUse } from "../examples/first-use.mjs";
import {
  DocumentEditor,
  editorDocument,
  formatEditorResult,
} from "../examples/editor.mjs";

await initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);

test("first-use caller keeps setup refusals separate and releases its owners", () => {
  firstUse(); // warm complete preparation before recording arena retention
  const baseline = liveStorageOwners();
  const accepted = firstUse({ id: 7 });
  assert.equal(accepted.status, "checked");
  assert.equal(accepted.result.outcome, "satisfies");
  assert.equal(accepted.operations[0].key, "lookup");
  assert.deepEqual(accepted.operations[0].aliases, ["find"]);
  assert.equal(
    firstUse({ id: 7 }, exampleDocument, "lookup").result.outcome,
    "satisfies",
  );
  assert.equal(firstUse({ id: 0 }).result.outcome, "fails");
  const admission = firstUse({ id: NaN }).result;
  assert.equal(admission.outcome, "input-error");
  assert.equal(admission.error.instancePointer, "/id");
  assert.equal(
    firstUse({}, exampleDocument, "missing").status,
    "operation-missing",
  );
  assert.equal(firstUse({}, "{").status, "input-error");
  const invalid = firstUse(
    {},
    '{"openbindings":"0.2.0","operations":{"lookup":{"description":42}}}',
  );
  assert.equal(invalid.status, "report");
  assert.equal(invalid.report.conclusion, "non-conformant");
  assert.equal(liveStorageOwners(), baseline);
});

test("source editor requires conformance and retains prepared partial snapshots", () => {
  const corrected = editorDocument.replace('"inputSchema":', '"input":');
  const warm = new DocumentEditor();
  warm.update(corrected);
  warm.dispose();
  const baseline = liveStorageOwners();
  const editor = new DocumentEditor();
  let retained;
  try {
    const rejected = editor.update(editorDocument);
    assert.equal(rejected.status, "report");
    assert.equal(rejected.report.conclusion, "non-conformant");
    const finding = rejected.report.findings.find(
      (item) => item.location?.pointer === "/operations/lookup/inputSchema",
    );
    assert.equal(finding.code, "schema-mismatch");
    assert.equal(finding.message, "value violates the fixed normative schema");
    assert.match(formatEditorResult(rejected), /UTF-8 byte column/);
    assert.equal(editor.snapshot(), undefined);

    const accepted = editor.update(corrected);
    assert.equal(accepted.status, "updated");
    assert.equal(accepted.result.outcome, "satisfies");
    retained = editor.snapshot();
    assert.equal(new TextDecoder().decode(retained.toBytes()), corrected);
    assert.equal(editor.update("{").status, "input-error");
    assert.equal(
      editor.update(
        '{"openbindings":"0.2.0","operations":{"lookup":{"aliases":["find"]}}}',
      ).status,
      "no-value-contract",
    );
    const partialText =
      '{"openbindings":"0.2.0","operations":{"lookup":{"aliases":["find"],"input":{"$ref":"https://schema.example/missing"}}}}';
    const partial = editor.update(partialText);
    assert.equal(partial.status, "updated");
    assert.equal(partial.result.outcome, "no-verdict");
    assert.equal(partial.result.detail.reason, "resource-unavailable");
    assert.match(formatEditorResult(partial), /resource-unavailable/);
    const declined = editor.update(
      '{"openbindings":"0.2.0","operations":{"lookup":{"aliases":["find"],"input":{"not":{"$ref":"https://schema.example/missing"}}}}}',
    );
    assert.equal(declined.status, "resources-missing");
    assert.equal(
      declined.evidence.message,
      "static preparation requires a resource that was not supplied",
    );
    const current = editor.snapshot();
    try {
      assert.equal(new TextDecoder().decode(current.toBytes()), partialText);
    } finally {
      current.dispose();
    }
    editor.dispose();
    assert.equal(retained.operations[0].key, "lookup");
  } finally {
    retained?.dispose();
    editor.dispose();
  }
  assert.equal(liveStorageOwners(), baseline);
});

test("editor presentation quotes original pointers and reports diagnostic truncation", () => {
  const editor = new DocumentEditor();
  try {
    const key = 'bad/~\n\u001b<img src=x onerror="fail()">';
    const text = JSON.stringify({
      openbindings: "0.2.0",
      operations: { lookup: { [key]: true } },
    });
    const result = editor.update(text);
    assert.equal(result.status, "report");
    const finding = result.report.findings.find(
      (item) => item.code === "schema-mismatch",
    );
    assert.equal(
      finding.location.pointer,
      `/operations/lookup/${key.replaceAll("~", "~0").replaceAll("/", "~1")}`,
    );
    const rendered = formatEditorResult(result);
    assert(rendered.includes(JSON.stringify(finding.location.pointer)));
    assert(!rendered.includes("\u001b"));
    // Rendering uses actual findings; this branch models a report that hit its bound.
    const truncated = formatEditorResult({
      ...result,
      report: { ...result.report, findingsTruncated: true },
    });
    assert.match(truncated, /More findings were omitted/);
    const mismatch = formatEditorResult({
      status: "updated",
      operations: [{ key: "lookup" }],
      result: {
        outcome: "fails",
        problems: [
          {
            instance: { pointer: "/id", byteOffset: 6, line: 1, byteColumn: 7 },
            schema: {
              resource: null,
              pointer: "/operations/lookup/input",
            },
            message:
              "value does not satisfy the constraint at the schema location",
          },
        ],
        problemsComplete: false,
      },
    });
    assert.match(mismatch, /Instance "\/id"/);
    assert.match(mismatch, /Selected input diagnostics complete: false/);
  } finally {
    editor.dispose();
  }
});
