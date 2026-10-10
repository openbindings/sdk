import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import * as bridge from "../dist/wasm/openbindings_wasm.js";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const exact = (text) => {
  const r = sdk.parseJson(text);
  assert.equal(r.status, "parsed");
  return r.value;
};
const parsed = (text) => {
  const r = sdk.parseDocument(text);
  assert.equal(r.status, "parsed");
  return r.value;
};
const disposed = (call) =>
  assert.throws(
    call,
    (e) => e instanceof sdk.SdkError && e.code === "disposed-handle",
  );
const interpretation = (call) =>
  assert.throws(
    call,
    (e) => e instanceof sdk.SdkError && e.code === "interpretation",
  );

test("raw member cursor preserves source order, duplicates, escaped/unpaired names and independent ownership", () => {
  const baseline = sdk.liveStorageOwners();
  const root = exact(
    '{"z":9007199254740993,"\\ud800":null,"z":0.29000000000000001,"a/~":false}',
  );
  const cursor = root.members();
  root.dispose();
  const rows = [],
    retained = [];
  try {
    for (const member of cursor) {
      const name = member.name,
        value = member.value,
        again = member.retain();
      try {
        rows.push([
          member.index,
          name.text,
          value.text,
          name.metadata.location.byteOffset,
        ]);
        retained.push(value.retain());
        member.dispose();
        assert.equal(again.index, rows.length - 1);
        const repeatedName = again.name;
        try {
          assert.equal(repeatedName.text, name.text);
        } finally {
          repeatedName.dispose();
        }
      } finally {
        name.dispose();
        value.dispose();
        again.dispose();
        member.dispose();
      }
    }
    assert.deepEqual(
      rows.map((r) => r.slice(0, 3)),
      [
        [0, '"z"', "9007199254740993"],
        [1, '"\\ud800"', "null"],
        [2, '"z"', "0.29000000000000001"],
        [3, '"a/~"', "false"],
      ],
    );
    assert.notEqual(rows[0][3], rows[2][3]);
    assert.equal(cursor.disposed, true);
    cursor.dispose();
    assert.deepEqual(cursor.next(), { done: true, value: undefined });
    assert.equal(retained[0].text, "9007199254740993");
  } finally {
    cursor.dispose();
    retained.forEach((v) => v.dispose());
    root.dispose();
  }
  assert.equal(sdk.liveStorageOwners(), baseline);
});

test("member/element cursors preserve the first terminal reason for every closing order", () => {
  const baseline = sdk.liveStorageOwners();
  for (const source of ["{}", "[]"]) {
    const root = exact(source),
      iterator = source === "{}" ? root.members() : root.elements();
    root.dispose();
    assert.equal(iterator.next().done, true);
    iterator.dispose();
    iterator.return();
    iterator.dispose();
    assert.equal(iterator.disposed, true);
    assert.equal(iterator.next().done, true);
  }
  for (const method of ["members", "elements"])
    for (const terminal of ["return", "dispose"]) {
      const root = exact(method === "members" ? ' {"x":1} ' : "[1]"),
        iterator = root[method]();
      iterator[terminal]();
      root.dispose();
      assert.equal(iterator.disposed, true);
      iterator.return();
      iterator.dispose();
      if (terminal === "return") assert.equal(iterator.next().done, true);
      else disposed(() => iterator.next());
    }
  for (const source of ["null", "1", "false", '"x"']) {
    const root = exact(source);
    assert.equal(root.members(), undefined);
    assert.equal(root.elements(), undefined);
    root.dispose();
    disposed(() => root.members());
    disposed(() => root.elements());
  }
  assert.equal(sdk.liveStorageOwners(), baseline);
});

test("array iteration is shallow, supports early break/throw and retained children after parent release", () => {
  const baseline = sdk.liveStorageOwners();
  const root = exact("[9007199254740993,[[[null]]],0.29000000000000001]");
  const iterator = root.elements();
  root.dispose();
  let retained;
  for (const child of iterator) {
    try {
      retained = child.retain();
      break;
    } finally {
      child.dispose();
    }
  }
  assert.equal(iterator.disposed, true);
  assert.equal(iterator.next().done, true);
  assert.equal(retained.text, "9007199254740993");
  retained.dispose();
  iterator.dispose();
  const nested = exact("[".repeat(2500) + "null" + "]".repeat(2500));
  const one = nested.elements();
  nested.dispose();
  const child = one.next().value;
  one.dispose();
  assert.equal(child.metadata.kind, "array");
  child.dispose();
  const other = exact('{"first":{},"unvisited":[1,2,3]}'),
    members = other.members();
  other.dispose();
  assert.throws(() => {
    for (const entry of members) {
      try {
        throw new Error("stop");
      } finally {
        entry.dispose();
      }
    }
  }, /stop/);
  assert.equal(members.disposed, true);
  assert.equal(members.next().done, true);
  members.dispose();
  assert.equal(sdk.liveStorageOwners(), baseline);
});

test("cursor failure closes only cursor; metadata-only prefix never acquires unvisited child owners", () => {
  const baseline = sdk.liveStorageOwners();
  const root = exact('{"first":1,"second":2}'),
    iterator = root.members();
  root.dispose();
  const first = iterator.next().value;
  const proto = bridge.WasmMembers.prototype,
    next = proto.next;
  try {
    proto.next = () => {
      throw new Error("injected next");
    };
    assert.throws(() => iterator.next(), /injected next/);
    assert.equal(iterator.disposed, true);
    assert.equal(iterator.return().done, true);
    disposed(() => iterator.next());
    assert.equal(first.index, 0);
  } finally {
    proto.next = next;
    first.dispose();
    iterator.dispose();
  }
  let steps = 0;
  proto.next = function () {
    steps++;
    return next.call(this);
  };
  const big = exact('{"first":0,"tail":[' + "0,".repeat(20000) + "0]}");
  const prefix = big.members();
  big.dispose();
  try {
    const only = prefix.next().value;
    only.dispose();
    prefix.return();
    assert.equal(steps, 1);
  } finally {
    proto.next = next;
    prefix.dispose();
  }
  assert.equal(sdk.liveStorageOwners(), baseline);
});

const documentText =
  '{"openbindings":"0.2.0","description":"before","operations":{"__proto__":{"tags":[],"aliases":["alias"],"input":{"const":9007199254740993},"examples":{"constructor":{"input":9007199254740993,"output":null,"x-exact":0.29000000000000001}}},"constructor":{}},"schemas":{"__proto__":{"enum":[9007199254740992,9007199254740993]},"constructor":true},"sources":{"toString":{"kind":"local","content":null}},"bindings":{"constructor":{"operation":"__proto__","source":"toString","content":{"large":1000000000000000128},"preference":-1,"deprecated":false}},"dependencies":{"__proto__":{"operation":"__proto__","kinds":[]}},"x-exact":1000000000000000128,"additionalFields":false}';

test("native parsed-to-editable draft roundtrip, direct edits and prototype-sensitive names preserve exact unrelated values", () => {
  const original = parsed(documentText),
    originalValue = original.value;
  const converted = original.toDraft();
  assert.equal(converted.status, "drafted");
  const editing = converted.draft,
    draft = editing.value;
  assert.equal(editing.value, draft);
  original.dispose();
  try {
    assert.equal(Object.hasOwn(draft.operations, "__proto__"), true);
    assert.equal(Object.getPrototypeOf(draft.operations), Object.prototype);
    assert.equal(Object.hasOwn(draft.schemas, "constructor"), true);
    assert.equal(
      draft.operations.__proto__.input instanceof sdk.ExactJson,
      true,
    );
    draft.description = "after";
    draft.operations.temporary = { input: { type: "integer" } };
    draft.operations.__proto__.tags.push("reviewed");
    draft.operations.__proto__.tags.pop();
    draft.operations.__proto__.aliases.push("another");
    draft.operations.__proto__.aliases.pop();
    draft.dependencies.__proto__.kinds.push("x");
    draft.dependencies.__proto__.kinds.pop();
    const added = sdk.authorDocument(draft);
    assert.equal(added.status, "authored");
    try {
      const found = added.document.resolveOperation("temporary");
      assert.equal(found.status, "found");
      found.operation.dispose();
    } finally {
      // Separate acquisition to cover ownership of selected handles.
      const selected = added.document.resolveOperation("temporary");
      if (selected.status === "found") selected.operation.dispose();
      added.document.dispose();
    }
    delete draft.operations.temporary;
    const result = sdk.authorDocument(draft);
    assert.equal(result.status, "authored");
    const newValue = result.document.value;
    try {
      const cursor = originalValue.members();
      try {
        for (const member of cursor) {
          const name = member.name,
            before = member.value;
          try {
            const decoded = name.toValue();
            assert.equal(decoded.status, "converted");
            if (decoded.value === "description") continue;
            const after = newValue.get(decoded.value);
            try {
              assert.equal(before.equals(after), true, decoded.value);
            } finally {
              after.dispose();
            }
          } finally {
            name.dispose();
            before.dispose();
            member.dispose();
          }
        }
      } finally {
        cursor.dispose();
      }
      disposed(() => original.originalBytes);
    } finally {
      newValue.dispose();
      result.document.dispose();
    }
  } finally {
    editing.dispose();
    originalValue.dispose();
    original.dispose();
  }
});

test("draft registry cleans removed/replaced leaves, retains explicit escapes, and never adopts external owners", () => {
  const baseline = sdk.liveStorageOwners();
  const source = parsed(
    '{"openbindings":"0.2.0","operations":{"run":{"input":true}},"schemas":{"kept":9007199254740993}}',
  );
  const converted = source.toDraft();
  assert.equal(converted.status, "drafted");
  const editing = converted.draft,
    draft = editing.value;
  const removed = draft.schemas.kept,
    retained = removed.retain(),
    external = exact("0.29000000000000001");
  source.dispose();
  delete draft.schemas.kept;
  draft.operations.run.input = external;
  const branch = sdk.authorDocument(draft);
  assert.equal(branch.status, "authored");
  const other = branch.document.toDraft();
  assert.equal(other.status, "drafted");
  branch.document.dispose();
  editing.dispose();
  try {
    disposed(() => removed.text);
    disposed(() => editing.value);
    disposed(() => sdk.authorDocument(draft));
    assert.equal(retained.text, "9007199254740993");
    assert.equal(external.text, "0.29000000000000001");
    assert.equal(
      other.draft.value.operations.run.input.text,
      "0.29000000000000001",
    );
    const plain = { ...draft, operations: {} };
    const result = sdk.authorDocument(plain);
    assert.equal(result.status, "authored");
    result.document.dispose();
  } finally {
    other.draft.dispose();
    external.dispose();
    retained.dispose();
    editing.dispose();
    source.dispose();
  }
  assert.equal(sdk.liveStorageOwners(), baseline);
});

test("draft conversion errors are located native refusals, independent of conformance", () => {
  for (const [text, code, pointer] of [
    [
      '{"openbindings":"0.2.0","operations":{},"description":17}',
      "invalid-field",
      "/description",
    ],
    [
      '{"openbindings":"0.2.0","operations":{"\\ud800":{}}}',
      "invalid-field",
      null,
    ],
    [
      '{"openbindings":"0.2.0","operations":{},"x":1,"x":2}',
      "duplicate-members",
      "",
    ],
    ['{"openbindings":"0.2.0"}', "invalid-field", ""],
  ]) {
    const source = parsed(text);
    try {
      const result = source.toDraft();
      assert.equal(result.status, "authoring-error");
      assert.equal(result.error.code, code);
      assert.equal(result.error.draftPointer, null);
      assert.equal(result.error.sourceLocation.pointer, pointer);
      assert.equal(typeof result.error.sourceLocation.byteOffset, "number");
      assert.deepEqual(source.originalBytes, new TextEncoder().encode(text));
    } finally {
      source.dispose();
    }
  }
  for (const [text, conclusion] of [
    [
      '{"openbindings":"0.2.0","operations":{"run":{"input":null}},"unexpected":{"value":9007199254740993}}',
      "non-conformant",
    ],
    [
      '{"openbindings":"0.2.0","operations":{"run":{"input":null}},"unexpected":{"\\ud800":9007199254740993}}',
      "undetermined",
    ],
  ]) {
    const source = parsed(text);
    const before = source.assess().report;
    assert.equal(before.conclusion, conclusion);
    const converted = source.toDraft();
    assert.equal(converted.status, "drafted");
    source.dispose();
    try {
      const result = sdk.authorDocument(converted.draft.value);
      assert.equal(result.status, "authored");
      try {
        assert.equal(result.document.assess().report.conclusion, conclusion);
      } finally {
        result.document.dispose();
      }
    } finally {
      converted.draft.dispose();
    }
  }
});

test("draft transfer partial failures release taken and untaken exact roots", () => {
  const source = parsed(documentText),
    baseline = sdk.liveStorageOwners();
  const proto = bridge.WasmDraft.prototype,
    originalShape = proto.shape,
    originalTake = proto.takeExact;
  try {
    for (const failure of ["shape", "decode", "second-leaf"]) {
      let calls = 0;
      proto.shape =
        failure === "shape"
          ? () => {
              throw new Error("shape fault");
            }
          : failure === "decode"
            ? () => "{"
            : originalShape;
      proto.takeExact = function (index) {
        if (failure === "second-leaf" && ++calls === 2)
          throw new Error("leaf fault");
        return originalTake.call(this, index);
      };
      assert.throws(() => source.toDraft());
      assert.equal(sdk.liveStorageOwners(), baseline);
    }
  } finally {
    proto.shape = originalShape;
    proto.takeExact = originalTake;
    source.dispose();
  }
});

test("new metadata is narrow/frozen/lexical; exact views retain after document release", () => {
  const source = parsed(documentText);
  const binding = source.binding("constructor"),
    sourceView = source.source("toString"),
    dependency = source.dependency("__proto__");
  const selected = source.resolveOperation("alias");
  assert.equal(selected.status, "found");
  const operation = selected.operation,
    example = operation.example("constructor");
  try {
    assert.deepEqual(
      source.operations.map((x) => x.key),
      ["__proto__", "constructor"],
    );
    assert.equal(source.operations[0].deprecated, null);
    assert.deepEqual(source.operations[0].tags, []);
    assert.equal(Object.isFrozen(source.operations[0].tags), true);
    assert.equal(Object.isFrozen(binding.metadata), true);
    assert.equal("content" in binding.metadata, false);
    assert.equal(binding.metadata.preference, -1);
    assert.equal(binding.metadata.deprecated, false);
    assert.equal(Object.isFrozen(dependency.metadata.kinds), true);
    assert.deepEqual(dependency.metadata.kinds, []);
    assert.deepEqual(operation.examples, [
      {
        key: "constructor",
        description: null,
        hasInput: true,
        hasOutput: true,
      },
    ]);
    const content = sourceView.content,
      input = example.input,
      output = example.output,
      retained = binding.retain();
    source.dispose();
    operation.dispose();
    example.dispose();
    binding.dispose();
    sourceView.dispose();
    dependency.dispose();
    try {
      assert.equal(content.text, "null");
      assert.equal(input.text, "9007199254740993");
      assert.equal(output.text, "null");
      assert.equal(retained.metadata.source, "toString");
      disposed(() => binding.metadata);
    } finally {
      content.dispose();
      input.dispose();
      output.dispose();
      retained.dispose();
    }
  } finally {
    source.dispose();
    operation.dispose();
    example.dispose();
    binding.dispose();
    sourceView.dispose();
    dependency.dispose();
  }
});

test("keyed metadata checks selected rows and avoids operation namespace interpretation", () => {
  const source = parsed(
    '{"openbindings":"0.2.0","operations":{"broken":{"aliases":[17]}},"sources":{"good":{"kind":"k"},"bad":{"kind":"k","description":17}},"bindings":{"good":{"operation":"absent","source":"absent"}},"dependencies":{"good":{"operation":"absent"},"bad":{"operation":"x","kinds":[17]}}}',
  );
  try {
    interpretation(() => source.operations);
    const good = source.source("good"),
      bad = source.source("bad"),
      binding = source.binding("good"),
      dependency = source.dependency("good");
    try {
      assert.equal(good.metadata.kind, "k");
      interpretation(() => bad.metadata);
      interpretation(() => source.sources);
      assert.equal(binding.metadata.source, "absent");
      assert.equal(source.source("absent"), undefined);
      assert.equal(dependency.metadata.kinds, null);
      assert.equal(source.dependencyAcceptsKind("good", "arbitrary"), true);
      interpretation(() => source.dependencyAcceptsKind("bad", "arbitrary"));
      interpretation(() => source.dependencies);
    } finally {
      good.dispose();
      bad.dispose();
      binding.dispose();
      dependency.dispose();
    }
  } finally {
    source.dispose();
  }
  for (const [suffix, expected] of [
    ["", null],
    [',"sources":{},"bindings":{},"dependencies":{}', []],
  ]) {
    const doc = parsed(
      '{"openbindings":"0.2.0","operations":{"run":{}}' + suffix + "}",
    );
    try {
      assert.deepEqual(doc.sources, expected);
      assert.deepEqual(doc.bindings, expected);
      assert.deepEqual(doc.dependencies, expected);
    } finally {
      doc.dispose();
    }
  }
  const malformed = parsed(
    '{"openbindings":"0.2.0","operations":{},"sources":null}',
  );
  try {
    interpretation(() => malformed.sources);
    interpretation(() => malformed.source("x"));
  } finally {
    malformed.dispose();
  }
});
