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
  const opaque = String.raw`{"openbindings":"0.2.0","operations":{},"x-editor":{
    "annotation":null,"large":900719925474099312345,"tiny":1e-1000,
    "negative":-0,"é":1e500,"e\u0301":2,"__proto__":{"value":3.00}}}`;
  for (let attempt = 0; attempt < 25; attempt++) {
    const changed = example.editExtensionMember(
      opaque,
      "x-editor",
      "annotation",
      "Reviewed",
    );
    assert.equal(changed.status, "edited");
    assert.equal(changed.report.conclusion, "conformant");
    const opened = sdk.parseDocument(changed.bytes);
    assert.equal(opened.status, "parsed");
    const root = opened.value.value;
    try {
      for (const [key, token] of [
        ["large", "900719925474099312345"],
        ["tiny", "1e-1000"],
        ["negative", "-0"],
        ["é", "1e500"],
        ["e\u0301", "2"],
        ["__proto__/value", "3.00"],
        ["annotation", '"Reviewed"'],
      ]) {
        const value = root.at(`/x-editor/${key}`);
        try {
          assert.equal(value.text, token);
        } finally {
          value?.dispose();
        }
      }
    } finally {
      root.dispose();
      opened.value.dispose();
    }
    // Exercise each refusal after some children have been acquired as well.
    for (const text of [
      "null",
      '{"other":0}',
      '{"x":0,"x":1}',
      '{"x":0,"nested":{"a":1,"a":2}}',
    ]) {
      const parsed = sdk.parseJson(text);
      assert.equal(parsed.status, "parsed");
      try {
        assert.throws(
          () => example.replaceExactObjectMember(parsed.value, "x", true),
          TypeError,
          text,
        );
        assert.equal(parsed.value.text, text);
      } finally {
        parsed.value.dispose();
      }
    }
    // Unlike a Rust UTF-8 string, a JavaScript string can carry a lone UTF-16
    // unit. Checked conversion and composition preserve that member name.
    const unpaired = sdk.parseJson(String.raw`{"\ud800":1,"x":0}`);
    assert.equal(unpaired.status, "parsed");
    try {
      const edited = example.replaceExactObjectMember(
        unpaired.value,
        "x",
        true,
      );
      try {
        assert.deepEqual(example.previewMembers(edited, 2), [
          { index: 0, name: String.raw`"\ud800"`, value: "1" },
          { index: 1, name: '"x"', value: "true" },
        ]);
      } finally {
        edited.dispose();
      }
    } finally {
      unpaired.value.dispose();
    }
    assert.equal(sdk.liveStorageOwners(), baseline);
  }
  assert.equal(
    example.editExtensionMember(opaque, "x-absent", "annotation", true).status,
    "extension-missing",
  );
  // A built snapshot survives both its converted-draft owner and a newly
  // composed caller-owned leaf. Replaced old leaves remain scope-owned.
  const source = sdk.parseDocument(opaque);
  assert.equal(source.status, "parsed");
  const conversion = source.value.toDraft();
  assert.equal(conversion.status, "drafted");
  let built;
  try {
    const inserted = example.replaceExactObjectMember(
      conversion.draft.value.additionalFields["x-editor"],
      "annotation",
      "Detached",
    );
    try {
      conversion.draft.value.additionalFields["x-editor"] = inserted;
      built = sdk.authorDocument(conversion.draft.value);
    } finally {
      inserted.dispose();
    }
  } finally {
    conversion.draft.dispose();
    source.value.dispose();
  }
  assert.equal(built.status, "authored");
  const detached = built.document.value;
  try {
    const member = detached.at("/x-editor/annotation");
    try {
      assert.equal(member.text, '"Detached"');
    } finally {
      member.dispose();
    }
  } finally {
    detached.dispose();
    built.document.dispose();
  }
  assert.equal(sdk.liveStorageOwners(), baseline);
  return {
    installedInspectionEditingExample: "passed",
    releasedArenas: baseline,
  };
}
