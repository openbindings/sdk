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
      many(document.operations()).map((value) => value.key),
      ["a", "z"],
    );
    assert.equal(document.operation("alias"), undefined);
    const operation = own(document.resolveOperation("alias"));
    assert.ok(operation instanceof sdk.Operation);
    assert.equal(operation.key, "z");
    assert.equal(operation.description, "operation");
    assert.equal(operation.deprecated, false);
    assert.deepEqual(operation.tags, ["second", "first"]);
    assert.deepEqual(operation.aliases, ["alias"]);
    assert.ok(Object.isFrozen(operation.tags));
    assert.equal(own(own(operation.input()).json()).text, "true");
    assert.equal(own(own(operation.output()).json()).text, "false");
    assert.deepEqual(
      many(operation.examples()).map((value) => value.key),
      ["null", "sample"],
    );
    const example = own(operation.example("sample"));
    assert.equal(example.description, "example");
    assert.equal(own(example.input()).text, "9007199254740993");
    assert.equal(own(example.output()).text, "0.29000000000000001");
    const source = own(document.source("s"));
    assert.equal(source.kind, "example.Kind@1");
    assert.equal(source.description, "source");
    assert.equal(own(source.data()).text, "null");
    const binding = own(document.binding("z"));
    assert.equal(binding.description, "binding");
    assert.equal(binding.preference, -1);
    assert.equal(binding.idempotent, false);
    assert.equal(binding.deprecated, true);
    assert.equal(binding.operationKey, "z");
    assert.equal(binding.sourceKey, "s");
    assert.equal(own(binding.operation()).key, "z");
    assert.equal(own(binding.source()).kind, "example.Kind@1");
    assert.equal(own(own(binding.data()).get("wide")).text, "9007199254740993");
    assert.deepEqual(
      many(operation.bindings()).map((value) => value.key),
      ["a", "z"],
    );
    const dependency = own(document.dependency("d"));
    assert.equal(dependency.description, "dependency");
    assert.equal(dependency.operationKey, "z");
    assert.equal(own(dependency.operation()).key, "z");
    assert.deepEqual(dependency.kinds, ["example.Kind@1", "other"]);
    assert.equal(dependency.acceptsKind("example.Kind@1"), true);
    assert.equal(dependency.acceptsKind("example.kind@1"), false);
    assert.equal(many(document.sources()).length, 1);
    assert.equal(many(document.bindings()).length, 2);
    assert.equal(many(document.dependencies()).length, 1);
    const schemas = document.schemas();
    many(schemas.values());
    assert.deepEqual([...schemas.keys()], ["a", "z"]);
    assert.equal(own(own(document.schema("z")).json()).text, "false");
  }));
test("canonical owners keep same-snapshot relationships and operation-local examples", () =>
  owners((own) => {
    const document = own(proof(fixture)),
      binding = own(document.binding("z"));
    const dependency = own(document.dependency("d")),
      operation = own(binding.operation());
    const example = own(operation.example("sample")),
      other = own(own(document.operation("a")).example("sample"));
    const draft = own(document.asDocument());
    const changed = own(
      draft.edit([
        { op: "set", pointer: "/sources/s/kind", value: "changed" },
        { op: "set", pointer: "/operations/z/description", value: "changed" },
      ]),
    );
    const next = own(changed.validate().document);
    assert.equal(own(own(next.binding("z")).source()).kind, "changed");
    next.dispose();
    changed.dispose();
    draft.dispose();
    document.dispose();
    operation.dispose();
    assert.equal(own(binding.source()).kind, "example.Kind@1");
    assert.equal(own(dependency.operation()).description, "operation");
    assert.equal(own(other.output()).text, '"another"');
    assert.equal(
      own(example.value()).metadata.location.pointer,
      "/operations/z/examples/sample",
    );
    assert.equal(
      own(other.value()).metadata.location.pointer,
      "/operations/a/examples/sample",
    );
    assert.equal(own(own(example.value()).get("x-exact")).text, "1.00");
    const output = own(example.output());
    example.dispose();
    assert.equal(output.text, "0.29000000000000001");
    assert.equal(own(binding.document()).name, "interface");
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
    const operation = own(absent.operation("op"));
    for (const field of ["description", "deprecated", "tags", "aliases"])
      assert.equal(operation[field], undefined);
    assert.equal(operation.examples(), undefined);
    assert.equal(operation.input(), undefined);
    assert.equal(operation.output(), undefined);
    assert.deepEqual(many(operation.bindings()), []);
    const empty = own(
      proof(
        '{"openbindings":"0.2.0","operations":{"op":{"deprecated":false,"tags":[],"aliases":[],"examples":{}}},"sources":{},"bindings":{},"dependencies":{},"schemas":{}}',
      ),
    );
    for (const method of ["sources", "bindings", "dependencies"])
      assert.deepEqual(many(empty[method]()), []);
    assert.equal(empty.schemas().size, 0);
    const op = own(empty.operation("op"));
    assert.equal(op.deprecated, false);
    assert.deepEqual(op.tags, []);
    assert.deepEqual(op.aliases, []);
    assert.deepEqual(many(op.examples()), []);
  }));
