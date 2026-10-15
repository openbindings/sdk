import * as testing from "../dist/testing.js";
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
  const r = sdk.Document.parse(text);
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
  const baseline = testing.liveStorageOwners();
  const root = exact(
    '{"z":9007199254740993,"\\ud800":null,"z":0.29000000000000001,"a/~":false}',
  );
  const cursor = root.members();
  root.dispose();
  const rows = [],
    retained = [];
  try {
    for (const member of cursor) {
      const name = member.name(),
        value = member.value(),
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
        const repeatedName = again.name();
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
  assert.equal(testing.liveStorageOwners(), baseline);
});

test("member/element cursors preserve the first terminal reason for every closing order", () => {
  const baseline = testing.liveStorageOwners();
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
  assert.equal(testing.liveStorageOwners(), baseline);
});

test("array iteration is shallow, supports early break/throw and retained children after parent release", () => {
  const baseline = testing.liveStorageOwners();
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
  assert.equal(testing.liveStorageOwners(), baseline);
});

test("cursor failure closes only cursor; metadata-only prefix never acquires unvisited child owners", () => {
  const baseline = testing.liveStorageOwners();
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
  assert.equal(testing.liveStorageOwners(), baseline);
});

const documentText =
  '{"openbindings":"0.2.0","description":"before","operations":{"__proto__":{"tags":[],"aliases":["alias"],"input":{"const":9007199254740993},"examples":{"constructor":{"input":9007199254740993,"output":null,"x-exact":0.29000000000000001}}},"constructor":{}},"schemas":{"__proto__":{"enum":[9007199254740992,9007199254740993]},"constructor":true},"sources":{"toString":{"kind":"local","content":null}},"bindings":{"constructor":{"operation":"__proto__","source":"toString","content":{"large":1000000000000000128},"preference":-1,"deprecated":false}},"dependencies":{"__proto__":{"operation":"__proto__","kinds":[]}},"x-exact":1000000000000000128,"additionalFields":false}';

test("document edits and prototype-sensitive names preserve exact unrelated values", () => {
  // DOC-1: edits operate directly on exact documents.
  const original = parsed(documentText),
    originalValue = original.value();
  const document = original.retain();
  original.dispose();
  try {
    document.setDescription("after");
    document.setOperation("temporary", { input: { type: "integer" } });
    document.set("/operations/__proto__/tags", ["reviewed"]);
    document.set("/operations/__proto__/tags", []);
    document.set("/operations/__proto__/aliases/-", "another");
    document.remove("/operations/__proto__/aliases/1");
    document.set("/dependencies/__proto__/kinds/-", "x");
    document.remove("/dependencies/__proto__/kinds/0");
    const found = document.resolveOperation("temporary");
    assert.equal(found.status, "found");
    found.operation.dispose();
    document.remove("/operations/temporary");
    const newValue = document.value(),
      cursor = originalValue.members();
    try {
      for (const member of cursor) {
        const name = member.name(),
          before = member.value();
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
      disposed(() => original.toBytes());
    } finally {
      cursor.dispose();
      newValue.dispose();
    }
  } finally {
    originalValue.dispose();
    document.dispose();
  }
});

test("edits retain removed leaves and never adopt external owners", () => {
  // DOC-1: every acquired exact value owns its revision independently.
  const baseline = testing.liveStorageOwners();
  const source = parsed(
    '{"openbindings":"0.2.0","operations":{"run":{"input":true}},"schemas":{"kept":9007199254740993}}',
  );
  const root = source.value(),
    removed = root.at("/schemas/kept"),
    retained = removed.retain(),
    external = exact("0.29000000000000001");
  root.dispose();
  source.remove("/schemas/kept");
  source.setOperationInput("run", external);
  const branch = source.retain();
  source.dispose();
  try {
    disposed(() => source.setName("disposed"));
    assert.equal(removed.text, "9007199254740993");
    assert.equal(retained.text, "9007199254740993");
    assert.equal(external.text, "0.29000000000000001");
    const value = branch.value(),
      input = value.at("/operations/run/input");
    try {
      assert.equal(input.text, external.text);
    } finally {
      input.dispose();
      value.dispose();
    }
  } finally {
    branch.dispose();
    removed.dispose();
    retained.dispose();
    external.dispose();
  }
  assert.equal(testing.liveStorageOwners(), baseline);
});

test("malformed documents stay inspectable and repairable independently of conformance", () => {
  // DOC-1: typed read errors address current bytes; duplicates refuse editing.
  const source = parsed(
    '{"openbindings":"0.2.0","operations":{},"description":17}',
  );
  assert.throws(
    () => source.description,
    (e) => e.code === "interpretation" && e.location.pointer === "/description",
  );
  source.setDescription("repaired");
  const proof = source.validate();
  assert.equal(proof.status, "conformant");
  proof.document.dispose();
  source.dispose();
  for (const text of [
    '{"openbindings":"0.2.0","operations":{"\\ud800":{}}}',
    '{"openbindings":"0.2.0"}',
  ]) {
    const document = parsed(text);
    assert.equal(document.validate().status, "report");
    if (!text.includes("operations"))
      assert.throws(() => document.operations, sdk.SdkError);
    assert.deepEqual(document.toBytes(), new TextEncoder().encode(text));
    document.setOperations({});
    const checked = document.validate();
    assert.equal(checked.status, "conformant");
    checked.document.dispose();
    document.dispose();
  }
  const duplicate = parsed(
    '{"openbindings":"0.2.0","operations":{},"x":1,"x":2}',
  );
  const before = duplicate.toBytes();
  assert.throws(
    () => duplicate.setName("refused"),
    (e) => e.code === "duplicate-members",
  );
  assert.deepEqual(duplicate.toBytes(), before);
  duplicate.dispose();
  for (const text of [
    '{"openbindings":"0.2.0","operations":{"run":{"input":null}},"unexpected":{"value":9007199254740993}}',
    '{"openbindings":"0.2.0","operations":{"run":{"input":null}},"unexpected":{"\\ud800":9007199254740993}}',
  ]) {
    const document = parsed(text),
      held = document.retain();
    document.dispose();
    held.setDescription("still a broken document");
    assert.equal(held.validate().report.conclusion, "non-conformant");
    held.dispose();
  }
});

test("failed edit encoding and native commits release temporary state", () => {
  // DOC-1: failure leaves the document unchanged and releases temporary owners.
  const source = parsed(documentText),
    baseline = testing.liveStorageOwners(),
    before = source.toBytes();
  const original = bridge.WasmDocument.prototype.editEncoded;
  try {
    const sentinel = new Error("commit fault");
    bridge.WasmDocument.prototype.editEncoded = () => {
      throw sentinel;
    };
    assert.throws(
      () => source.edit([{ op: "set", pointer: "/name", value: "new" }]),
      (e) => e === sentinel,
    );
    assert.deepEqual(source.toBytes(), before);
    assert.equal(testing.liveStorageOwners(), baseline);
  } finally {
    bridge.WasmDocument.prototype.editEncoded = original;
  }
  try {
    assert.throws(() =>
      source.edit([
        { op: "set", pointer: "/name", value: "new" },
        { op: "set", pointer: "/bad", value: undefined },
      ]),
    );
    assert.deepEqual(source.toBytes(), before);
    assert.equal(testing.liveStorageOwners(), baseline);
  } finally {
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
    const content = sourceView.content(),
      input = example.input(),
      output = example.output(),
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
