import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import * as sdk from "../dist/index.js";
import * as discovery from "../dist/http-discovery.js";
import { prepareInput } from "./conformant-contracts.mjs";
await sdk.initialize(
  await readFile(
    new URL("../dist/wasm/openbindings_wasm_bg.wasm", import.meta.url),
  ),
);
const MIB = 1024 * 1024;
const DEFAULT_BYTES = 32 * MIB;
const NO_DEPTH_CAP = 4294967295;
// A conformant document of exactly `bytes`, padded by one string.
const head = '{"openbindings":"0.2.0","operations":{},"x-fill":"';
const padded = (bytes) => head + "a".repeat(bytes - head.length - 2) + '"}';
const empty = '{"openbindings":"0.2.0","operations":{}}';
const exceeded = (bytes, limit) =>
  `the document's ${bytes} bytes exceed the ${limit}-byte admission limit`;
const editRefusal = (limit, rust, typescript) =>
  `document edit refused: the edited document exceeds its ${limit} limit; ` +
  `admit the document with a larger JsonLimits::${rust} (limits.${typescript} in TypeScript)`;
const parsed = (result) => {
  assert.equal(result.status, "parsed");
  return result.value;
};
// Every rule is inconclusive at the parser offset, with its specific code.
const undetermined = (report, message, code, byteOffset) => {
  assert.equal(report.conclusion, "undetermined");
  assert.equal(report.findings.length, 14);
  for (const finding of report.findings) {
    assert.equal(finding.code, code);
    assert.deepEqual(finding.location, {
      byteOffset,
      line: 1,
      byteColumn: byteOffset + 1,
      pointer: null,
    });
    assert.doesNotMatch(finding.message, /validate|JsonLimits|TypeScript/);
    assert.equal(finding.message, message);
  }
};
// A document limit refusal carries Rust's code and message, and no part
// pointer, on every route.
const refusedEdit = (call, code, message) =>
  assert.throws(
    call,
    (e) =>
      e instanceof sdk.EditError &&
      e.code === code &&
      e.message === message &&
      e.inputPointer === null,
  );

test("validateDocument admits 32 MiB and locates the byte limit one byte past it", () => {
  const within = sdk.validateDocument(padded(DEFAULT_BYTES));
  assert.equal(within.status, "report");
  assert.equal(within.report.conclusion, "conformant");
  undetermined(
    sdk.validateDocument(padded(DEFAULT_BYTES + 1)).report,
    exceeded(DEFAULT_BYTES + 1, DEFAULT_BYTES),
    "byte-limit",
    DEFAULT_BYTES,
  );
});

test("a raised byte limit admits documents and values to it and refuses one byte more", () => {
  const limit = DEFAULT_BYTES + 1024,
    limits = { maxBytes: limit },
    at = padded(limit),
    over = padded(limit + 1);
  assert.equal(
    sdk.validateDocument(at, { limits }).report.conclusion,
    "conformant",
  );
  undetermined(
    sdk.validateDocument(over, { limits }).report,
    exceeded(limit + 1, limit),
    "byte-limit",
    limit,
  );
  const document = parsed(sdk.Document.parse(at, { limits }));
  assert.deepEqual(document.limits, {
    maxBytes: limit,
    maxNodes: 1_000_000,
    maxDepth: 10_000,
  });
  assert.ok(Object.isFrozen(document.limits));
  document.dispose();
  const refused = sdk.Document.parse(over, { limits });
  assert.equal(refused.status, "input-error");
  assert.equal(refused.error.code, "byte-limit");
  assert.equal(refused.error.byteOffset, limit);
  assert.match(
    refused.error.message,
    /; raise JsonLimits::max_bytes \(limits\.maxBytes in TypeScript\)$/,
  );
  parsed(sdk.parseJson(at, { limits })).dispose();
  assert.equal(sdk.parseJson(over, { limits }).error.code, "byte-limit");
  assert.equal(sdk.parseJson(at).error.code, "byte-limit");
});

test("unspecified limits keep the JsonLimits defaults, and malformed limits throw", () => {
  const deep = `{"openbindings":"0.2.0","operations":{},"x-d":${"[".repeat(10_001)}0${"]".repeat(10_001)}}`;
  // validateDocument without limits sets no nesting cap; with them, the
  // default 10,000 levels apply unless maxDepth raises them.
  assert.equal(sdk.validateDocument(deep).report.conclusion, "conformant");
  undetermined(
    sdk.validateDocument(deep, { limits: {} }).report,
    "the document nests containers deeper than the 10000 levels admitted",
    "depth-limit",
    sdk.Document.parse(deep).error.byteOffset,
  );
  assert.equal(
    sdk.validateDocument(deep, { limits: { maxDepth: NO_DEPTH_CAP } }).report
      .conclusion,
    "conformant",
  );
  assert.equal(sdk.Document.parse(deep).error.code, "depth-limit");
  parsed(sdk.Document.parse(deep, { limits: { maxDepth: 10_002 } })).dispose();
  assert.equal(
    sdk.validateDocument(empty, { limits: undefined }).report.conclusion,
    "conformant",
  );
  const invalid = (e) =>
    e instanceof sdk.SdkError && e.code === "invalid-json-limits";
  for (const limits of [
    null,
    [],
    [1],
    "x",
    5,
    new Map(),
    new Date(),
    Object.defineProperty({}, "maxBytes", { get: () => 1, enumerable: true }),
    Object.defineProperty({}, "maxBytes", { value: 1, enumerable: false }),
    { [Symbol("s")]: 1 },
    { maxBytes: -1 },
    { maxBytes: 1.5 },
    { maxNodes: 2 ** 32 },
    { maxDepth: null },
    { depth: 1 },
  ]) {
    assert.throws(() => sdk.validateDocument(empty, { limits }), invalid);
    assert.throws(() => sdk.Document.parse(empty, { limits }), invalid);
    assert.throws(() => sdk.parseJson(empty, { limits }), invalid);
  }
  for (const options of [null, [], "x", { limit: { maxBytes: 1 } }])
    assert.throws(() => sdk.validateDocument(empty, options), invalid);
  parsed(
    sdk.parseJson(empty, Object.assign(Object.create(null), { limits: {} })),
  ).dispose();
});

test("edits on a document admitted with raised limits parse under them, and every route names the setting", () => {
  const limits = { maxBytes: DEFAULT_BYTES + 1024 };
  const large = parsed(
    sdk.parseJson(`"${"a".repeat(DEFAULT_BYTES)}"`, { limits }),
  );
  let grown = parsed(sdk.Document.parse(empty, { limits }));
  grown = replaceDocument(grown, grown.set("/x-fill", large));
  assert.ok(grown.toBytes().length > DEFAULT_BYTES);
  grown = replaceDocument(
    grown,
    grown.edit([{ op: "set", pointer: "/x-other", value: { n: 1 } }]),
  );
  grown = replaceDocument(grown, grown.set("/description", "d".repeat(512)));
  const validation = grown.validate();
  assert.equal(validation.status, "conformant");
  assert.deepEqual(validation.document.limits, grown.limits);
  validation.document.dispose();
  // An ordinary value past the default bytes encodes for this document.
  grown = replaceDocument(grown, grown.remove("/x-fill"));
  grown = replaceDocument(
    grown,
    grown.set("/description", "e".repeat(DEFAULT_BYTES)),
  );
  assert.ok(grown.toBytes().length > DEFAULT_BYTES);
  // A document parsed with the defaults refuses each of them, by every route,
  // with Rust's edit refusal.
  const plain = parsed(sdk.Document.parse(empty));
  const bytes = editRefusal("byte", "max_bytes", "maxBytes");
  refusedEdit(() => plain.set("/x-fill", large), "byte-limit", bytes);
  refusedEdit(
    () => plain.edit([{ op: "set", pointer: "/x-fill", value: large }]),
    "byte-limit",
    bytes,
  );
  refusedEdit(
    () => plain.set("/operations/" + "o", { input: large }),
    "byte-limit",
    bytes,
  );
  refusedEdit(
    () => plain.set("/description", "e".repeat(DEFAULT_BYTES)),
    "byte-limit",
    bytes,
  );
  assert.equal(plain.toBytes().length, empty.length);
  for (const owner of [large, grown, plain]) owner.dispose();
});

test("edit limits apply to what the edit writes, as in Rust, not to the edit's own structure", () => {
  // A document admitted at 4 levels holds a 3-level value at a top-level
  // member, whichever route writes it; one level more refuses.
  let shallow = parsed(sdk.Document.parse(empty, { limits: { maxDepth: 4 } }));
  const three = parsed(sdk.parseJson("[[[1]]]"));
  shallow = replaceDocument(shallow, shallow.set("/x-a", three));
  shallow = replaceDocument(shallow, shallow.remove("/x-a"));
  shallow = replaceDocument(shallow, shallow.set("/x-a", [[[1]]]));
  shallow = replaceDocument(shallow, shallow.remove("/x-a"));
  shallow = replaceDocument(
    shallow,
    shallow.edit([{ op: "set", pointer: "/x-a", value: three }]),
  );
  shallow = replaceDocument(
    shallow,
    shallow.edit([{ op: "set", pointer: "/x-a", value: [[[1]]] }]),
  );
  shallow = replaceDocument(
    shallow,
    shallow.set("/operations/" + "o", { input: { type: "string" } }),
  );
  const nesting = editRefusal("nesting", "max_depth", "maxDepth");
  refusedEdit(() => shallow.set("/x-b", [[[[1]]]]), "depth-limit", nesting);
  // Values deeper than the limit on their own refuse before crossing the
  // bridge, the same way.
  refusedEdit(() => shallow.set("/x-b", [[[[[1]]]]]), "depth-limit", nesting);
  refusedEdit(
    () => shallow.set("/operations/" + "p", { input: [[[[[1]]]]] }),
    "depth-limit",
    nesting,
  );
  refusedEdit(
    () => shallow.set("/x-b", parsed(sdk.parseJson("[[[[1]]]]"))),
    "depth-limit",
    nesting,
  );
  // A document admitted at 7 nodes takes one more member and value.
  let few = parsed(sdk.Document.parse(empty, { limits: { maxNodes: 7 } }));
  few = replaceDocument(few, few.set("/x-a", 1));
  few = replaceDocument(few, few.remove("/x-a"));
  few = replaceDocument(few, few.set("/x-a", parsed(sdk.parseJson("1"))));
  few = replaceDocument(few, few.remove("/x-a"));
  few = replaceDocument(
    few,
    few.edit([{ op: "set", pointer: "/x-a", value: 1 }]),
  );
  refusedEdit(
    () => few.set("/x-b", 1),
    "node-limit",
    editRefusal("node", "max_nodes", "maxNodes"),
  );
  // A document admitted at its own size takes a same-size replacement.
  const exact = '{"openbindings":"0.2.0","operations":{},"x-a":"abc"}';
  let sized = parsed(
    sdk.Document.parse(exact, { limits: { maxBytes: exact.length } }),
  );
  sized = replaceDocument(sized, sized.set("/x-a", "xyz"));
  refusedEdit(
    () => sized.set("/x-a", "wxyz"),
    "byte-limit",
    editRefusal("byte", "max_bytes", "maxBytes"),
  );
  for (const owner of [shallow, three, few, sized]) owner.dispose();
});

test("edit rows hold only their op's members, before any limit applies", () => {
  let tight = parsed(
    sdk.Document.parse(empty, {
      limits: { maxBytes: 100, maxNodes: 20, maxDepth: 5 },
    }),
  );
  const malformed = (row, code) =>
    assert.throws(
      () => tight.edit([row]),
      (e) => e instanceof sdk.EditError && e.code === code,
    );
  malformed(
    { op: "set", pointer: "/x-a", value: 1, extra: 2 },
    "invalid-edits",
  );
  malformed(
    { op: "set", pointer: "/x-a", value: 1, extra: "x".repeat(1_000_000) },
    "invalid-edits",
  );
  malformed(
    { op: "remove", pointer: "/operations", value: "x".repeat(1000) },
    "invalid-edits",
  );
  malformed({ op: "move", pointer: "/x-a" }, "invalid-edits");
  malformed(
    { op: "set-operations", value: {}, pointer: "/x" },
    "invalid-edits",
  );
  malformed({ op: "set", pointer: 7, value: 1 }, "invalid-pointer");
  malformed({ op: "remove" }, "invalid-pointer");
  assert.equal(new TextDecoder().decode(tight.toBytes()), empty);
  tight = replaceDocument(
    tight,
    tight.edit([{ op: "set", pointer: "/x-a", value: 1 }]),
  );
  tight = replaceDocument(
    tight,
    tight.edit([{ op: "remove", pointer: "/x-a" }]),
  );
  assert.equal(new TextDecoder().decode(tight.toBytes()), empty);
  tight.dispose();
});

test("a projection keeps its own bounds whatever the document was admitted under", () => {
  // Each empty allOf member projects to a reference and a definition, so the
  // projection is many times the document; admitting the document at its own
  // size does not bound it.
  const text = JSON.stringify({
    openbindings: "0.2.0",
    operations: { op: { input: { allOf: Array(20_000).fill({}) } } },
  });
  for (const limits of [{ maxBytes: text.length }, {}]) {
    const document = parsed(sdk.Document.parse(text, { limits }));
    const ready = prepareInput(document, "op");
    assert.equal(ready.status, "ready");
    ready.schema.dispose();
    document.dispose();
  }
  // A document admitted past the default byte limit prepares a contract whose
  // projection passes it, and decides a value.
  const literal = `"${"a".repeat(DEFAULT_BYTES)}"`;
  const limits = { maxBytes: 2 * DEFAULT_BYTES };
  const large = parsed(
    sdk.Document.parse(
      `{"openbindings":"0.2.0","operations":{"op":{"input":{"const":${literal}}}}}`,
      { limits },
    ),
  );
  const prepared = prepareInput(large, "op");
  assert.equal(prepared.status, "ready");
  const value = parsed(sdk.parseJson(literal, { limits }));
  assert.equal(prepared.schema.evaluate(value).status, "satisfies");
  for (const owner of [large, prepared.schema, value]) owner.dispose();
});

test("discovery with a raised body limit validates a body past the default byte limit", async () => {
  const body = padded(DEFAULT_BYTES + 1);
  const fetch = async () => new Response(body);
  const found = await discovery.discover("https://example.test", {
    fetch,
    maxDocumentBytes: DEFAULT_BYTES + 1,
  });
  assert.equal(found.status, "found");
  assert.deepEqual(found.document.limits, {
    maxBytes: DEFAULT_BYTES + 1,
    maxNodes: 1_000_000,
    maxDepth: NO_DEPTH_CAP,
  });
  found.document.dispose();
  const limited = await discovery.discover("https://example.test", {
    fetch,
    maxDocumentBytes: DEFAULT_BYTES,
  });
  assert.equal(limited.status, "body-limit");
  // A body past the node limit is undetermined with a located node-limit.
  // Validating it again with larger limits admits it.
  const nodes = `{"openbindings":"0.2.0","operations":{},"x-a":[${Array(1_000_000).fill("0").join(",")}]}`;
  const many = await discovery.discover("https://example.test", {
    fetch: async () => new Response(nodes),
    maxDocumentBytes: 8 * MIB,
  });
  assert.equal(many.status, "undetermined");
  undetermined(
    many.report,
    "the document exceeds the admission limit of 1000000 JSON nodes",
    "node-limit",
    sdk.Document.parse(nodes).error.byteOffset,
  );
  assert.equal(
    sdk.validateDocument(nodes, {
      limits: { maxNodes: 2_000_000, maxDepth: NO_DEPTH_CAP },
    }).report.conclusion,
    "conformant",
  );
});

test("a document reports the limits it keeps", () => {
  const defaults = {
    maxBytes: DEFAULT_BYTES,
    maxNodes: 1_000_000,
    maxDepth: 10_000,
  };
  let created = sdk.Document.create();
  assert.deepEqual(created.limits, defaults);
  created = replaceDocument(created, created.set("/name", "kept"));
  assert.deepEqual(created.limits, defaults);
  created.dispose();
  const parsedDefault = parsed(sdk.Document.parse(empty));
  assert.deepEqual(parsedDefault.limits, defaults);
  parsedDefault.dispose();
});

test("ExactJson.from refuses an ordinary value past the nesting limit with a RangeError", () => {
  const nest = (levels) => {
    let value = [];
    for (let i = 1; i < levels; i++) value = [value];
    return value;
  };
  sdk.ExactJson.from(nest(10_000)).dispose();
  assert.throws(
    () => sdk.ExactJson.from(nest(10_001)),
    (e) =>
      e instanceof RangeError &&
      e.message ===
        "Ordinary value exceeds the 10000-level nesting limit; parse it with parseJson and a larger limits.maxDepth.",
  );
  // Multibyte text past the byte limit that the encoder's character count
  // admits is refused by admission, also as a RangeError.
  assert.throws(
    () => sdk.ExactJson.from("é".repeat(DEFAULT_BYTES / 2 + 1)),
    (e) =>
      e instanceof RangeError &&
      e.message ===
        "The ordinary value exceeds the admitted JSON limits; parse it with parseJson and larger limits.",
  );
});

// Test fixture cursor: keep the new immutable snapshot and release its predecessor.
function replaceDocument(previous, next) {
  previous.dispose();
  return next;
}

test("each conformance admission limit preserves its parser offset without API advice", () => {
  const text = '{"openbindings":"0.3.0",\n"operations":{},"x-a":[0]}';
  for (const [limits, code] of [
    [{ maxBytes: 30 }, "byte-limit"],
    [{ maxDepth: 1 }, "depth-limit"],
    [{ maxNodes: 5 }, "node-limit"],
  ]) {
    const parser = sdk.Document.parse(text, { limits }).error;
    const report = sdk.validateDocument(text, { limits }).report;
    assert.equal(report.conclusion, "undetermined");
    assert.equal(report.findings.length, 14);
    for (const finding of report.findings) {
      assert.equal(finding.code, code);
      assert.equal(finding.location.byteOffset, parser.byteOffset);
      const prefix = text.slice(0, parser.byteOffset);
      assert.equal(finding.location.line, prefix.split("\n").length);
      assert.equal(
        finding.location.byteColumn,
        prefix.length - prefix.lastIndexOf("\n"),
      );
      assert.equal(finding.location.pointer, null);
      assert.doesNotMatch(finding.message, /validate|JsonLimits|TypeScript/);
    }
  }
});
