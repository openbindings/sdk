import test from "node:test";
import assert from "node:assert/strict";
import * as sdk from "../dist/index.js";
await sdk.initialize();

test("exact diagnostic pointer JSON keeps a lone unit and its selected member", () => {
  const value = sdk.parseJson('{"\\udc00":1,"\\ufffd":2}').value;
  const pointer = value.locate("/\udc00").pointer;
  const text = JSON.stringify(pointer);
  assert.equal(text, '"/\\udc00"');
  const restored = JSON.parse(text);
  const exact = value.at(restored);
  // Rust's ordinary Serde form is deliberately lossy; JS must not adopt it.
  const ordinary = value.at(JSON.parse('"/�"'));
  try {
    assert.equal(restored, pointer);
    assert.equal(exact.text, "1");
    assert.equal(ordinary.text, "2");
  } finally {
    ordinary.dispose();
    exact.dispose();
    value.dispose();
  }
});

test("diagnostic pointer from Wasm also round trips through exact JSON text", () => {
  const draft = sdk.Document.parse(
    '{"schemas":{"\\udc00":{"$ref":"#/schemas/target"},"target":true}}',
  ).value;
  try {
    const report = draft.references();
    assert.equal(report.complete, true);
    const pointer = report.references[0].location.pointer;
    assert.equal(pointer, "/schemas/\udc00/$ref");
    assert.equal(JSON.parse(JSON.stringify(pointer)), pointer);
  } finally {
    draft.dispose();
  }
});
