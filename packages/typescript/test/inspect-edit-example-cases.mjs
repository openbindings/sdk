import assert from "node:assert/strict";

// Runs against exports from the installed package and its verbatim TS example.
export function inspectEditExampleCases(sdk, example) {
  const text = `{
    "openbindings":"0.2.0",
    "operations":{"read":{"aliases":["fetch"],"tags":["read"],
      "output":{"type":"integer"},
      "examples":{"large":{"output":900719925474099312345}}}},
    "sources":{"archive":{"kind":"https://example.test/archive",
      "content":{"limit":900719925474099312345}}},
    "bindings":{"native":{"operation":"read","source":"archive","preference":0}},
    "x-tokens":{"large":900719925474099312345,"negative":-0}
  }`;
  const inspect = () => example.inspectBindings(text, "fetch");
  const edit = () => example.editMetadata(text, "Reviewed", "read", "reviewed");
  inspect();
  edit(); // warm fixed conformance state before observing released arenas
  const baseline = sdk.liveStorageOwners();
  const inventory = inspect();
  assert.equal(inventory.status, "inspected");
  assert.equal(inventory.operation.key, "read");
  assert.equal(inventory.rows[0].binding.preference, 0);
  assert.equal(inventory.rows[0].source.kind, "https://example.test/archive");
  assert.equal(example.inspectBindings(text, "absent").status, "missing");
  assert.equal(example.inspectBindings("{", "read").status, "input-error");
  const missingSource = text.replace(
    '"source":"archive"',
    '"source":"missing"',
  );
  assert.equal(
    example.inspectBindings(missingSource, "read").rows[0].source,
    null,
  );
  const edited = edit();
  assert.equal(edited.status, "edited");
  assert.equal(edited.report.conclusion, "conformant");
  const before = sdk.parseDocument(text);
  const after = sdk.parseDocument(edited.bytes);
  assert.equal(before.status, "parsed");
  assert.equal(after.status, "parsed");
  try {
    const original = before.value.value;
    const revised = after.value.value;
    try {
      for (const pointer of [
        "/x-tokens",
        "/sources/archive/content",
        "/operations/read/examples/large/output",
      ]) {
        const oldValue = original.at(pointer);
        const newValue = revised.at(pointer);
        try {
          assert.equal(newValue.text, oldValue.text, pointer);
        } finally {
          newValue?.dispose();
          oldValue?.dispose();
        }
      }
      assert.equal(new TextDecoder().decode(before.value.originalBytes), text);
      assert.deepEqual(after.value.operations[0].tags, ["read", "reviewed"]);
    } finally {
      revised.dispose();
      original.dispose();
    }
  } finally {
    after.value.dispose();
    before.value.dispose();
  }
  assert.equal(
    example.editMetadata(text, "Reviewed", "absent", "x").status,
    "operation-missing",
  );
  assert.equal(
    example.editMetadata("{", "Reviewed", "read", "x").status,
    "input-error",
  );
  assert.equal(
    example.editMetadata(
      '{"openbindings":"0.2.0","operations":{"read":{"tags":42}}}',
      "Reviewed",
      "read",
      "x",
    ).status,
    "authoring-error",
  );
  const raw = sdk.parseJson(
    String.raw`{"a":900719925474099312345,"a":-0,"\ud800":true}`,
  );
  assert.equal(raw.status, "parsed");
  try {
    assert.deepEqual(example.previewMembers(raw.value, 2), [
      { index: 0, name: '"a"', value: "900719925474099312345" },
      { index: 1, name: '"a"', value: "-0" },
    ]);
    assert.equal(
      example.previewMembers(raw.value, 3)[2].name,
      String.raw`"\ud800"`,
    );
    assert.deepEqual(example.previewMembers(raw.value, 0), []);
    assert.throws(() => example.previewMembers(raw.value, -1), RangeError);
  } finally {
    raw.value.dispose();
  }
  const scalar = sdk.parseJson("null");
  assert.equal(scalar.status, "parsed");
  try {
    assert.equal(example.previewMembers(scalar.value), undefined);
  } finally {
    scalar.value.dispose();
  }
  assert.equal(example.addTag({ operations: {} }, "toString", "x"), false);
  assert.equal(sdk.liveStorageOwners(), baseline);
  return {
    installedInspectionEditingExample: "passed",
    releasedArenas: baseline,
  };
}
