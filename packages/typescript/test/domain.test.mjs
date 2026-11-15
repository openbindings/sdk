import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { liveStorageOwners } from "../dist/testing.js";
await sdk.initialize();
const fixture = await readFile(
  new URL(
    "../../../crates/openbindings/tests/fixtures/canonical-domain.json",
    import.meta.url,
  ),
  "utf8",
);
function proof(text) {
  const parsed = sdk.Document.parse(text);
  assert.equal(parsed.status, "parsed");
  try {
    const validation = parsed.value.validate();
    assert.equal(validation.status, "conformant");
    return validation.document;
  } finally {
    parsed.value.dispose();
  }
}
function owners(run) {
  const baseline = liveStorageOwners(),
    held = [];
  const own = (value) => {
    if (value) held.push(value);
    return value;
  };
  const many = (values) => {
    for (const value of values ?? []) own(value);
    return values;
  };
  try {
    run(own, many);
  } finally {
    for (const value of held.reverse()) value.dispose();
  }
  assert.equal(
    liveStorageOwners(),
    baseline,
    "all canonical owners release their backing storage",
  );
}
test("all 32 domain fields and their relationships match the Rust fixture", () =>
  owners((own, many) => {
    const document = own(proof(fixture));
    assert.equal(document.openbindings, "0.2.0");
    assert.equal(document.name, "interface");
    assert.equal(document.version, "release-label");
    assert.equal(document.description, "document");
    assert.deepEqual(
      document.operations().map((value) => value.key),
      ["a", "z"],
    );
    assert.equal(document.operation("alias"), undefined);
    const operation = document.resolveOperation("alias");
    assert.ok(Object.isFrozen(operation));
    assert.equal(operation.key, "z");
    assert.equal(operation.description, "operation");
    assert.equal(operation.deprecated, false);
    assert.deepEqual(operation.tags, ["second", "first"]);
    assert.deepEqual(operation.aliases, ["alias"]);
    assert.ok(Object.isFrozen(operation.tags));
    assert.equal(
      own(own(document.inputSchema(operation.key)).json()).text,
      "true",
    );
    assert.equal(
      own(own(document.outputSchema(operation.key)).json()).text,
      "false",
    );
    assert.deepEqual(
      document.examples(operation.key).map((value) => value.key),
      ["null", "sample"],
    );
    const example = document.example(operation.key, "sample");
    assert.equal(example.description, "example");
    assert.equal(
      own(document.exampleInput(operation.key, example.key)).text,
      "9007199254740993",
    );
    assert.equal(
      own(document.exampleOutput(operation.key, example.key)).text,
      "0.29000000000000001",
    );
    const source = document.source("s");
    assert.equal(source.kind, "example.Kind@1");
    assert.equal(source.description, "source");
    assert.equal(own(document.sourceData(source.key)).text, "null");
    const binding = document.binding("z");
    assert.equal(binding.description, "binding");
    assert.equal(binding.preference, -1);
    assert.equal(binding.idempotent, false);
    assert.equal(binding.deprecated, true);
    assert.equal(binding.operationKey, "z");
    assert.equal(binding.sourceKey, "s");
    assert.equal(document.operation(binding.operationKey).key, "z");
    assert.equal(document.source(binding.sourceKey).kind, "example.Kind@1");
    assert.equal(
      own(own(document.bindingData(binding.key)).get("wide")).text,
      "9007199254740993",
    );
    assert.deepEqual(
      document.operationBindings(operation.key).map((value) => value.key),
      ["a", "z"],
    );
    const dependency = document.dependency("d");
    assert.equal(dependency.description, "dependency");
    assert.equal(dependency.operationKey, "z");
    assert.equal(document.operation(dependency.operationKey).key, "z");
    assert.deepEqual(dependency.kinds, ["example.Kind@1", "other"]);
    assert.equal(sdk.acceptsKind(dependency, "example.Kind@1"), true);
    assert.equal(sdk.acceptsKind(dependency, "example.kind@1"), false);
    assert.equal(document.sources().length, 1);
    assert.equal(document.bindings().length, 2);
    assert.equal(document.dependencies().length, 1);
    const schemas = document.schemas();
    many(schemas.values());
    assert.deepEqual([...schemas.keys()], ["a", "z"]);
    assert.equal(own(own(document.schema("z")).json()).text, "false");
  }));
test("detached records and proof queries preserve snapshot relationships and local examples", () =>
  owners((own) => {
    const document = own(proof(fixture));
    const retained = own(document.retain());
    const binding = document.binding("z"),
      dependency = document.dependency("d");
    const operation = document.operation(binding.operationKey);
    const example = document.example(operation.key, "sample");
    const other = document.example("a", "sample");
    const draft = own(document.asDocument());
    const changed = own(
      draft.edit([
        { op: "set", pointer: "/sources/s/kind", value: "changed" },
        { op: "set", pointer: "/operations/z/description", value: "changed" },
      ]),
    );
    const next = own(changed.validate().document);
    assert.equal(next.source(binding.sourceKey).kind, "changed");
    assert.equal(next.operation(operation.key).description, "changed");
    next.dispose();
    changed.dispose();
    draft.dispose();
    document.dispose();
    assert.equal(operation.description, "operation");
    assert.equal(retained.source(binding.sourceKey).kind, "example.Kind@1");
    assert.equal(
      retained.operation(dependency.operationKey).description,
      "operation",
    );
    assert.equal(own(retained.exampleOutput("a", other.key)).text, '"another"');
    const exact = own(retained.exampleValue(operation.key, example.key));
    assert.equal(
      exact.metadata.location.pointer,
      "/operations/z/examples/sample",
    );
    assert.equal(
      own(retained.exampleValue("a", other.key)).metadata.location.pointer,
      "/operations/a/examples/sample",
    );
    assert.equal(own(exact.get("x-exact")).text, "1.00");
    const output = own(retained.exampleOutput(operation.key, example.key));
    assert.equal(retained.name, "interface");
    retained.dispose();
    assert.equal(output.text, "0.29000000000000001");
    assert.equal(binding.sourceKey, "s");
    assert.equal(example.description, "example");
    assert.throws(
      () => retained.source(binding.sourceKey),
      (e) => e.code === "disposed-handle",
    );
  }));
test("optional namespaces retain absence versus empty and metadata retains false versus absent", () =>
  owners((own, many) => {
    const absent = own(
      proof('{"openbindings":"0.2.0","operations":{"op":{}}}'),
    );
    assert.equal(absent.sources(), undefined);
    assert.equal(absent.bindings(), undefined);
    assert.equal(absent.dependencies(), undefined);
    assert.equal(absent.schemas(), undefined);
    const operation = absent.operation("op");
    for (const field of ["description", "deprecated", "tags", "aliases"])
      assert.equal(operation[field], undefined);
    assert.equal(absent.examples(operation.key), undefined);
    assert.equal(absent.inputSchema(operation.key), undefined);
    assert.equal(absent.outputSchema(operation.key), undefined);
    assert.deepEqual(absent.operationBindings(operation.key), []);
    const empty = own(
      proof(
        '{"openbindings":"0.2.0","operations":{"op":{"deprecated":false,"tags":[],"aliases":[],"examples":{}}},"sources":{},"bindings":{},"dependencies":{},"schemas":{}}',
      ),
    );
    for (const method of ["sources", "bindings", "dependencies"])
      assert.deepEqual(empty[method](), []);
    assert.equal(empty.schemas().size, 0);
    const op = empty.operation("op");
    assert.equal(op.deprecated, false);
    assert.deepEqual(op.tags, []);
    assert.deepEqual(op.aliases, []);
    assert.deepEqual(empty.examples(op.key), []);
  }));
