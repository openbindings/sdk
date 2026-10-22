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
// A conformant document of exactly `bytes`, padded by one string.
const head = '{"openbindings":"0.2.0","operations":{},"x-fill":"';
const padded = (bytes) => head + "a".repeat(bytes - head.length - 2) + '"}';
const empty = '{"openbindings":"0.2.0","operations":{}}';
const exceeded = (bytes, limit) =>
  `the document's ${bytes} bytes exceed the ${limit}-byte admission limit; ` +
  "raise JsonLimits::max_bytes (limits.maxBytes in TypeScript)";
const parsed = (result) => {
  assert.equal(result.status, "parsed");
  return result.value;
};
// Every rule inconclusive, each finding an unlocated input-limit with `message`.
const undetermined = (result, message) => {
  assert.equal(result.status, "report");
  assert.equal(result.report.conclusion, "undetermined");
  assert.equal(result.report.findings.length, 13);
  for (const finding of result.report.findings) {
    assert.equal(finding.code, "input-limit");
    assert.equal(finding.location, null);
    assert.equal(finding.message, message);
  }
};

test("validateDocument admits 32 MiB and names the byte limit's setting one byte past it", () => {
  const within = sdk.validateDocument(padded(DEFAULT_BYTES));
  assert.equal(within.status, "report");
  assert.equal(within.report.conclusion, "conformant");
  undetermined(
    sdk.validateDocument(padded(DEFAULT_BYTES + 1)),
    exceeded(DEFAULT_BYTES + 1, DEFAULT_BYTES),
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
    sdk.validateDocument(over, { limits }),
    exceeded(limit + 1, limit),
  );
  parsed(sdk.Document.parse(at, { limits })).dispose();
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

test("unspecified limits keep the JsonLimits defaults, including the nesting cap", () => {
  const deep = `{"openbindings":"0.2.0","operations":{},"x-d":${"[".repeat(10_001)}0${"]".repeat(10_001)}}`;
  // validateDocument without limits sets no nesting cap; with them, the
  // default 10,000 levels apply unless maxDepth raises them.
  assert.equal(sdk.validateDocument(deep).report.conclusion, "conformant");
  undetermined(
    sdk.validateDocument(deep, { limits: {} }),
    "the document nests containers deeper than the 10000 levels admitted; " +
      "raise JsonLimits::max_depth (limits.maxDepth in TypeScript)",
  );
  assert.equal(
    sdk.validateDocument(deep, { limits: { maxDepth: 4294967295 } }).report
      .conclusion,
    "conformant",
  );
  assert.equal(sdk.Document.parse(deep).error.code, "depth-limit");
  parsed(sdk.Document.parse(deep, { limits: { maxDepth: 10_002 } })).dispose();
  for (const limits of [
    { maxBytes: -1 },
    { maxBytes: 1.5 },
    { maxNodes: 2 ** 32 },
    { maxDepth: null },
    { depth: 1 },
  ])
    assert.throws(
      () => sdk.validateDocument(empty, { limits }),
      (e) => e instanceof sdk.SdkError && e.code === "invalid-json-limits",
    );
  assert.throws(
    () => sdk.parseJson(empty, null),
    (e) => e instanceof sdk.SdkError && e.code === "invalid-json-limits",
  );
});

test("edits on a document admitted with raised limits parse under them", () => {
  const limits = { maxBytes: DEFAULT_BYTES + 1024 };
  const large = parsed(
    sdk.parseJson(`"${"a".repeat(DEFAULT_BYTES)}"`, { limits }),
  );
  const grown = parsed(sdk.Document.parse(empty, { limits }));
  grown.set("/x-fill", large);
  assert.ok(grown.toBytes().length > DEFAULT_BYTES);
  grown.edit([{ op: "set", pointer: "/x-other", value: { n: 1 } }]);
  grown.setDescription("d".repeat(512));
  const validation = grown.validate();
  assert.equal(validation.status, "conformant");
  validation.document.dispose();
  // An ordinary value past the default bytes encodes for this document.
  grown.remove("/x-fill");
  grown.setDescription("e".repeat(DEFAULT_BYTES));
  assert.ok(grown.toBytes().length > DEFAULT_BYTES);
  // A document parsed with the defaults refuses both.
  const plain = parsed(sdk.Document.parse(empty));
  assert.throws(
    () => plain.set("/x-fill", large),
    (e) => e instanceof sdk.EditError && e.code === "byte-limit",
  );
  assert.throws(
    () => plain.setDescription("e".repeat(DEFAULT_BYTES)),
    (e) =>
      e instanceof sdk.EditError &&
      e.code === "authoring-limit" &&
      e.message.includes("parse the document with a larger limits.maxBytes"),
  );
  for (const owner of [large, grown, plain]) owner.dispose();
});

test("a projection stops at the limits its document was admitted under", () => {
  // Each empty allOf member projects to a reference and a definition, so the
  // projection is many times the document.
  const text = JSON.stringify({
    openbindings: "0.2.0",
    operations: { op: { input: { allOf: Array(20_000).fill({}) } } },
  });
  const tight = parsed(
    sdk.Document.parse(text, { limits: { maxBytes: text.length } }),
  );
  const refused = prepareInput(tight, "op");
  assert.equal(refused.status, "no-verdict");
  assert.equal(refused.detail.code, "program-byte-limit");
  assert.equal(
    refused.detail.message,
    `the schema projection exceeds the document's ${text.length}-byte admission limit; ` +
      "raise JsonLimits::max_bytes (limits.maxBytes in TypeScript) when admitting the document",
  );
  const roomy = parsed(sdk.Document.parse(text));
  const ready = prepareInput(roomy, "op");
  assert.equal(ready.status, "ready");
  ready.valueContract.dispose();
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
  assert.equal(prepared.valueContract.validate(value).outcome, "satisfies");
  for (const owner of [tight, roomy, large, prepared.valueContract, value])
    owner.dispose();
});

test("discovery with a raised body limit validates a body past the default byte limit", async () => {
  const body = padded(DEFAULT_BYTES + 1);
  const fetch = async () => new Response(body);
  const found = await discovery.discover("https://example.test", {
    fetch,
    maxDocumentBytes: DEFAULT_BYTES + 1,
  });
  assert.equal(found.status, "found");
  found.document.dispose();
  const limited = await discovery.discover("https://example.test", {
    fetch,
    maxDocumentBytes: DEFAULT_BYTES,
  });
  assert.equal(limited.status, "body-limit");
});
