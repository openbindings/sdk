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

test("unpaired problem pointers still address the exact instance member", () =>
  owning((own) => {
    const proof = own(
      conformant(
        '{"openbindings":"0.2.0","operations":{"op":{"input":{"additionalProperties":{"type":"string"}}}}}',
      ),
    );
    const ready = prepareInput(proof, "op");
    assert.equal(ready.status, "ready");
    const prepared = own(ready.schema);
    const value = own(exact(String.raw`{"\ud800":1}`));
    const result = prepared.explain(value);
    assert.equal(result.status, "fails");
    assert.equal(result.problems[0].instance, "/\ud800");
    assert.equal(own(value.at(result.problems[0].instance)).text, "1");
    assert.equal(value.locate(result.problems[0].instance).pointer, "/\ud800");
  }));

test("transitive missing resources are visible before preparation and located during evaluation", () =>
  owning((own) => {
    const proof = own(
      conformant(
        '{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"https://example.test/a"}}}}',
      ),
    );
    const supplied = own(
      exact('{"allOf":[{"$ref":"missing"},{"type":"integer"}]}'),
    );
    const resources = own(
      new sdk.ExternalSchemas([["https://example.test/a", supplied]]),
    );
    const report = proof.references({ resources });
    assert.equal(report.complete, true);
    assert.equal(report.references.length, 2);
    const missing = report.references[1];
    assert.deepEqual(missing.location, {
      sourceUri: "https://example.test/a",
      pointer: "/allOf/0/$ref",
    });
    assert.equal(missing.resolution.status, "unresolved");
    assert.equal(missing.resolution.detail.reason, "resource-unavailable");
    assert.equal(
      missing.resolution.detail.resourceUri,
      "https://example.test/missing",
    );
    const ready = prepareInput(proof, "op", { resources });
    assert.equal(ready.status, "ready");
    const prepared = own(ready.schema);
    const result = prepared.evaluate(1);
    assert.equal(result.status, "no-verdict");
    assert.equal(result.detail.reason, "resource-unavailable");
    assert.deepEqual(result.detail.location, missing.location);
  }));

test("explained problems locate the value's node and the document's keyword", () =>
  owning((own) => {
    const source =
      '{\n  "openbindings": "0.2.0",\n  "operations": {"op": {"input": {"properties": {"items": {"items": {"type": "integer"}}}}}}\n}';
    const proof = own(conformant(source));
    const ready = prepareInput(proof, "op");
    assert.equal(ready.status, "ready");
    const contract = own(ready.schema);
    const text = '{\n  "items": [\n    1,\n    "two"\n  ]\n}';
    const value = own(exact(text));
    assert.deepEqual(contract.evaluate(value).problems, []);
    const result = contract.explain(value);
    assert.equal(result.status, "fails");
    const [problem] = result.problems;
    assert.equal(problem.instance, "/items/1");
    // The value holds its text, so it finds the line and column when asked.
    const offset = text.indexOf('"two"');
    assert.deepEqual(value.locate(problem.instance), {
      pointer: "/items/1",
      byteOffset: offset,
      ...position(text, offset),
    });
    assert.equal(value.locate("/items/2"), null);
    const keyword = "/operations/op/input/properties/items/items/type";
    assert.deepEqual(problem.schema, { sourceUri: null, pointer: keyword });
    const at = source.indexOf('"integer"');
    assert.deepEqual(proof.locate(problem.schema), {
      pointer: keyword,
      byteOffset: at,
      ...position(source, at),
    });
    assert.equal(
      proof.locate({ sourceUri: null, pointer: "/operations/op/absent" }),
      null,
    );
    const empty = own(new sdk.ExternalSchemas());
    assert.equal(empty.locate(problem.schema), null);
    assert.throws(
      () => proof.locate({ pointer: keyword }),
      (error) =>
        error instanceof sdk.SdkError &&
        error.code === "invalid-schema-location",
    );
    const ordinary = contract.explain({ items: [1, "two"] });
    assert.equal(ordinary.problems[0].instance, "/items/1");
  }));

test("a subtree value is named from itself and located in its text", () =>
  owning((own) => {
    const proof = own(
      conformant(
        '{"openbindings":"0.2.0","operations":{"op":{"input":{"properties":{"n":{"type":"integer"}}}}}}',
      ),
    );
    const ready = prepareInput(proof, "op");
    assert.equal(ready.status, "ready");
    const contract = own(ready.schema);
    const text = '{"request": {\n  "n": "x"\n}}';
    const whole = own(exact(text));
    const request = own(whole.at("/request"));
    const [problem] = contract.explain(request).problems;
    assert.equal(problem.instance, "/n");
    const offset = text.indexOf('"x"');
    assert.deepEqual(request.locate(problem.instance), {
      pointer: "/n",
      byteOffset: offset,
      ...position(text, offset),
    });
    // A pointer can name a member whose name holds an unpaired unit.
    const lone = own(exact('{"\\ud800":[1,\n"x"]}'));
    assert.deepEqual(lone.locate("/\ud800/1"), {
      pointer: "/\ud800/1",
      byteOffset: 14,
      line: 2,
      byteColumn: 1,
    });
  }));

test("supplied resources locate their keywords, including through unpaired names", () =>
  owning((own) => {
    const uri = "https://example.test/schema";
    const resourceText =
      '{\n  "$id": "https://example.test/schema",\n  "minimum": 10,\n  "$defs": {"\\ud800": {"type": "string"}}\n}';
    const resource = own(exact(resourceText));
    const resources = own(new sdk.ExternalSchemas([[uri, resource]]));
    const proof = own(
      conformant(
        `{"openbindings":"0.2.0","operations":{"op":{"input":{"$ref":"${uri}"}}}}`,
      ),
    );
    const ready = prepareInput(proof, "op", { resources });
    assert.equal(ready.status, "ready");
    const contract = own(ready.schema);
    const value = own(exact("3"));
    const [problem] = contract.explain(value).problems;
    assert.deepEqual(problem.schema, { sourceUri: uri, pointer: "/minimum" });
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
    assert.deepEqual(resources.locate({ sourceUri: uri, pointer: lone }), {
      pointer: lone,
      byteOffset: offset,
      ...position(resourceText, offset),
    });
    assert.equal(
      resources.locate({ sourceUri: uri, pointer: "/$defs/\udc00/type" }),
      null,
    );
  }));

test("at selects the node locate reports, including through unpaired names", () =>
  owning((own) => {
    const text = '{"\\ud800": {"\\udc00x": [1, "two"]}, "a~/b": 3}';
    const value = own(exact(text));
    for (const pointer of ["", "/\ud800", "/\ud800/\udc00x/1", "/a~0~1b"]) {
      const node = own(value.at(pointer));
      assert.equal(
        node.metadata.location.byteOffset,
        value.locate(pointer).byteOffset,
      );
    }
    assert.equal(own(value.at("/\ud800/\udc00x/1")).text, '"two"');
    // A pointer naming no node selects nothing either way.
    for (const pointer of ["/\udc00", "/\ud800/\udc00x/2", "/\ud800/\udc00"]) {
      assert.equal(value.at(pointer), undefined);
      assert.equal(value.locate(pointer), null);
    }
    // A malformed pointer selects nothing, as it does without unpaired units.
    assert.equal(value.at("/\ud800/~2"), undefined);
    assert.equal(value.at("/a~2"), undefined);
    assert.equal(value.at("no-slash\ud800"), undefined);
  }));

test("parse failures forward UTF-8 line and byte column", () => {
  const result = sdk.Document.parse('{\n "é" 0}');
  assert.equal(result.status, "input-error");
  assert.equal(result.error.byteOffset, 8);
  assert.equal(result.error.line, 2);
  assert.equal(result.error.byteColumn, 7);
  // A byte-limit refusal is located at the limit, counted over the admitted prefix.
  const limited = sdk.Document.parse("[\n0]", { limits: { maxBytes: 3 } });
  assert.equal(limited.status, "input-error");
  assert.equal(limited.error.byteOffset, 3);
  assert.equal(limited.error.line, 2);
  assert.equal(limited.error.byteColumn, 2);
});

test("document retained bytes include caches shared with a proof", () =>
  owning((own) => {
    const parsed = sdk.Document.parse(
      '{"openbindings":"0.2.0","operations":{"op":{}}}',
    );
    assert.equal(parsed.status, "parsed");
    const document = own(parsed.value);
    const before = document.retainedBytes();
    assert.ok(before > document.toBytes().length);
    const result = document.validate();
    assert.equal(result.status, "conformant");
    const proof = own(result.document);
    const snapshot = own(proof.asDocument());
    assert.ok(document.retainedBytes() > before);
    assert.equal(document.retainedBytes(), snapshot.retainedBytes());
  }));
