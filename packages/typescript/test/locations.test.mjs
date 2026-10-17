import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import { prepareInput } from "./conformant-contracts.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const utf8 = (text) => new TextEncoder().encode(text);
// One-based line and byte column of a UTF-8 byte offset, counted independently.
function position(text, offset) {
  const before = new TextDecoder().decode(utf8(text).slice(0, offset));
  const lines = before.split("\n");
  return {
    line: lines.length,
    byteColumn: utf8(lines[lines.length - 1]).length + 1,
  };
}
function conformant(text) {
  const parsed = sdk.Document.parse(text);
  assert.equal(parsed.status, "parsed");
  try {
    const checked = parsed.value.validate();
    assert.equal(checked.status, "conformant");
    return checked.document;
  } finally {
    parsed.value.dispose();
  }
}
function exact(text) {
  const parsed = sdk.parseJson(text);
  assert.equal(parsed.status, "parsed");
  return parsed.value;
}
// Run a job with owners it creates, disposing them afterwards.
function owning(job) {
  const owners = [];
  try {
    return job((owner) => (owners.push(owner), owner));
  } finally {
    for (const owner of owners.reverse()) owner.dispose();
  }
}

test("explained problems locate the value's node and the document's keyword", () =>
  owning((own) => {
    const source =
      '{\n  "openbindings": "0.2.0",\n  "operations": {"op": {"input": {"properties": {"items": {"items": {"type": "integer"}}}}}}\n}';
    const proof = own(conformant(source));
    const ready = prepareInput(proof, "op");
    assert.equal(ready.status, "ready");
    const contract = own(ready.contract);
    const text = '{\n  "items": [\n    1,\n    "two"\n  ]\n}';
    const value = own(exact(text));
    assert.deepEqual(contract.validate(value).problems, []);
    const result = contract.explain(value);
    assert.equal(result.outcome, "fails");
    const [problem] = result.problems;
    const offset = text.indexOf('"two"');
    assert.deepEqual(problem.instance, {
      pointer: "/items/1",
      byteOffset: offset,
      ...position(text, offset),
    });
    const keyword = "/operations/op/input/properties/items/items/type";
    assert.deepEqual(problem.schema, { resource: null, pointer: keyword });
    const at = source.indexOf('"integer"');
    assert.deepEqual(proof.locate(problem.schema), {
      pointer: keyword,
      byteOffset: at,
      ...position(source, at),
    });
    assert.equal(
      proof.locate({ resource: null, pointer: "/operations/op/absent" }),
      null,
    );
    const empty = own(new sdk.SchemaResources());
    assert.equal(empty.locate(problem.schema), null);
    assert.throws(
      () => proof.locate({ pointer: keyword }),
      (error) =>
        error instanceof sdk.SdkError &&
        error.code === "invalid-schema-location",
    );
    // An ordinary value is located in its exact serialization.
    const ordinary = contract.explain({ items: [1, "two"] });
    assert.deepEqual(ordinary.problems[0].instance, {
      pointer: "/items/1",
      byteOffset: '{"items":[1,'.length,
      line: 1,
      byteColumn: '{"items":[1,'.length + 1,
    });
  }));

test("supplied resources locate their keywords, including through unpaired names", () =>
  owning((own) => {
    const uri = "https://example.test/schema";
    const resourceText =
      '{\n  "$id": "https://example.test/schema",\n  "minimum": 10,\n  "$defs": {"\\ud800": {"type": "string"}}\n}';
    const resource = own(exact(resourceText));
    const resources = own(new sdk.SchemaResources([[uri, resource]]));
    const proof = own(
      conformant(
        `{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"${uri}"}}}}`,
      ),
    );
    const ready = prepareInput(proof, "op", { resources });
    assert.equal(ready.status, "ready");
    const contract = own(ready.contract);
    const value = own(exact("3"));
    const [problem] = contract.explain(value).problems;
    assert.deepEqual(problem.schema, { resource: uri, pointer: "/minimum" });
    const at = resourceText.indexOf("10");
    assert.deepEqual(resources.locate(problem.schema), {
      pointer: "/minimum",
      byteOffset: at,
      ...position(resourceText, at),
    });
    assert.equal(proof.locate(problem.schema), null);
    // A JavaScript string holds the unpaired unit; the pointer crosses exactly.
    const lone = "/$defs/\ud800/type";
    const offset = resourceText.indexOf('"string"');
    assert.deepEqual(resources.locate({ resource: uri, pointer: lone }), {
      pointer: lone,
      byteOffset: offset,
      ...position(resourceText, offset),
    });
    assert.equal(
      resources.locate({ resource: uri, pointer: "/$defs/\udc00/type" }),
      null,
    );
  }));
